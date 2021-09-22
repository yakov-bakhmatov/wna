use std::cell::RefCell;
use std::ffi::OsStr;
use std::mem::MaybeUninit;
use std::ptr;
use std::ptr::addr_of_mut;
use std::sync::mpsc::{channel, Sender};
use std::thread;

use winapi::shared::minwindef::*;
use winapi::shared::windef::*;
use winapi::um::errhandlingapi::GetLastError;
use winapi::um::libloaderapi::GetModuleHandleW;
use winapi::um::shellapi::*;
use winapi::um::winuser::*;

use super::{Event, Icon};

const TASKBAR_ICON_ID: UINT = 1;
const NOTIFICATION_MESSAGE_ID: UINT = WM_USER + 1;

thread_local!(static WINDOW_LOOP_DATA: RefCell<Option<WindowLoopData>> = RefCell::new(None));

#[derive(Clone)]
struct WindowHandle {
    pub hwnd: HWND,
    pub hmenu: HMENU,
}

unsafe impl Send for WindowHandle {}
unsafe impl Sync for WindowHandle {}

struct WindowLoopData {
    pub handle: WindowHandle,
    pub event_sender: Sender<Event>,
}

pub struct Window {
    handle: Option<WindowHandle>,
    thread: Option<thread::JoinHandle<()>>,
}

impl Window {
    pub fn create(window_class_name: &str, event_sender: Sender<Event>) -> Result<Window, String> {
        let window_class_name = str_to_wchar_str(window_class_name);
        let (sender, receiver) = channel();
        let thread = thread::Builder::new()
            .name("wna-window-loop".into())
            .spawn(move || unsafe {
                match init_window(&window_class_name) {
                    Ok(w) => {
                        let _ = sender.send(Ok(w.clone()));
                        drop(sender);
                        WINDOW_LOOP_DATA.with(|data| {
                            (*data.borrow_mut()) = Some(WindowLoopData {
                                handle: w,
                                event_sender,
                            });
                        });
                        window_message_loop();
                    }
                    Err(e) => {
                        let _ = sender.send(Err(e));
                    }
                }
            })
            .map_err(|e| format!("Error starting window loop: {}", e))?;
        let handle = receiver
            .recv()
            .map_err(|e| format!("Error receiving window handle: {}", e))??;
        Ok(Window {
            handle: Some(handle),
            thread: Some(thread),
        })
    }

    pub fn set_icon(&self, icon: &Icon) -> Result<(), String> {
        if let Some(ref handle) = self.handle {
            unsafe {
                let hicon = match icon {
                    Icon::File(ref file_name) => load_icon_from_file(file_name),
                    Icon::ResourceByName(ref name) => load_icon_from_resource_by_name(name),
                    Icon::ResourceByOrd(ord) => load_icon_from_resource_by_ord(*ord),
                }?;
                set_icon(handle.hwnd, hicon)
            }
        } else {
            Err("Window is closed".to_string())
        }
    }

    pub fn set_tip(&self, tip: &str) -> Result<(), String> {
        if let Some(ref handle) = self.handle {
            unsafe { set_tip(handle.hwnd, tip) }
        } else {
            Err("Window is closed".to_string())
        }
    }

    pub fn add_menu_item(&self, id: u32, title: &str) -> Result<(), String> {
        if let Some(ref handle) = self.handle {
            unsafe { add_menu_item(handle.hmenu, id, title) }
        } else {
            Err("Window is closed".to_string())
        }
    }

    pub fn add_menu_separator(&self, id: u32) -> Result<(), String> {
        if let Some(ref handle) = self.handle {
            unsafe { add_menu_separator(handle.hmenu, id) }
        } else {
            Err("Window is closed".to_string())
        }
    }

    pub fn show_balloon(&self, title: &str, body: &str) -> Result<(), String> {
        if let Some(ref handle) = self.handle {
            unsafe { show_balloon(handle.hwnd, title, body) }
        } else {
            Err("Window is closed".to_string())
        }
    }

    pub fn close(&mut self) {
        if let Some(ref h) = self.handle {
            unsafe {
                PostMessageW(h.hwnd, WM_DESTROY, 0, 0);
            }
        }
        self.handle = None;
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    msg: UINT,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        NOTIFICATION_MESSAGE_ID => {
            match lparam as UINT {
                WM_LBUTTONUP | WM_RBUTTONUP => {
                    let mut p: POINT = POINT { x: 0, y: 0 };
                    if GetCursorPos(&mut p) == 0 {
                        return 0;
                    }
                    SetForegroundWindow(hwnd);
                    WINDOW_LOOP_DATA.with(|data| {
                        if let Some(ref data) = data.borrow().as_ref() {
                            TrackPopupMenu(data.handle.hmenu, 0, p.x, p.y, 0, hwnd, ptr::null());
                        }
                    });
                }
                NIN_BALLOONUSERCLICK => {
                    WINDOW_LOOP_DATA.with(|data| {
                        if let Some(ref data) = data.borrow().as_ref() {
                            if data.event_sender.send(Event::Balloon).is_err() {
                                // event loop is terminated; close the window
                                PostMessageW(hwnd, WM_DESTROY, 0, 0);
                            }
                        }
                    });
                }
                _ => {}
            }
            0
        }
        WM_DESTROY => {
            let _ = delete_notification_area_icon(hwnd);
            PostQuitMessage(0);
            0
        }
        WM_COMMAND => {
            let menu_id = wparam as u32;
            WINDOW_LOOP_DATA.with(|data| {
                if let Some(ref data) = data.borrow().as_ref() {
                    if data.event_sender.send(Event::Menu(menu_id)).is_err() {
                        // event loop is terminated; close the window
                        PostMessageW(hwnd, WM_DESTROY, 0, 0);
                    }
                }
            });
            0
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn str_to_wchar_str(s: &str) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    let mut result: Vec<u16> = OsStr::new(s).encode_wide().collect();
    result.push(0);
    result
}

fn copy_str_to_wchar_array(arr: &mut [u16], s: &str) {
    let s = str_to_wchar_str(s);
    let len = ::std::cmp::min(s.len(), arr.len() - 1);
    arr[0..len].copy_from_slice(&s[0..len]);
    arr[len] = 0;
}

unsafe fn register_class(class_name: &[u16]) -> Result<(), String> {
    let class: WNDCLASSW = WNDCLASSW {
        style: 0,
        lpfnWndProc: Some(window_proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: ptr::null_mut(),
        hIcon: LoadIconW(ptr::null_mut(), IDI_APPLICATION),
        hCursor: LoadCursorW(ptr::null_mut(), IDI_APPLICATION),
        hbrBackground: COLOR_WINDOW as HBRUSH,
        lpszMenuName: ptr::null_mut(),
        lpszClassName: class_name.as_ptr(),
    };
    if RegisterClassW(&class) == 0 {
        return Err(format!(
            "Error registering window class: {}",
            GetLastError()
        ));
    }
    Ok(())
}

unsafe fn create_window(class_name: &[u16]) -> Result<HWND, String> {
    let hwnd = CreateWindowExW(
        0,
        class_name.as_ptr(),
        class_name.as_ptr(),
        WS_OVERLAPPEDWINDOW,
        CW_USEDEFAULT,
        0,
        CW_USEDEFAULT,
        0,
        ptr::null_mut(),
        ptr::null_mut(),
        ptr::null_mut(),
        ptr::null_mut(),
    );
    if hwnd.is_null() {
        return Err(format!("Error creating window: {}", GetLastError()));
    }
    Ok(hwnd)
}

unsafe fn create_popup_menu() -> Result<HMENU, String> {
    let hmenu = CreatePopupMenu();
    if hmenu.is_null() {
        return Err(format!("Error creating popup menu: {}", GetLastError()));
    }
    let menu_info: MENUINFO = MENUINFO {
        cbSize: ::std::mem::size_of::<MENUINFO>() as u32,
        fMask: MIM_APPLYTOSUBMENUS | MIM_STYLE,
        dwStyle: 0,
        cyMax: 0,
        hbrBack: COLOR_MENU as HBRUSH,
        dwContextHelpID: 0,
        dwMenuData: 0,
    };
    if SetMenuInfo(hmenu, &menu_info) == 0 {
        return Err(format!("Error setting popup menu info: {}", GetLastError()));
    }
    Ok(hmenu)
}

unsafe fn make_notify_icon_data(hwnd: HWND) -> NOTIFYICONDATAW {
    let mut data: NOTIFYICONDATAW = ::std::mem::zeroed();
    data.cbSize = ::std::mem::size_of::<NOTIFYICONDATAW>() as u32;
    data.hWnd = hwnd;
    data.uID = TASKBAR_ICON_ID;
    data
}

unsafe fn create_notification_area_icon(hwnd: HWND) -> Result<(), String> {
    let mut data: NOTIFYICONDATAW = make_notify_icon_data(hwnd);
    data.uFlags = NIF_MESSAGE;
    data.uCallbackMessage = NOTIFICATION_MESSAGE_ID;
    if Shell_NotifyIconW(NIM_ADD, &mut data) == 0 {
        return Err(format!("Error adding taskbar icon: {}", GetLastError()));
    }
    Ok(())
}

unsafe fn delete_notification_area_icon(hwnd: HWND) -> Result<(), String> {
    let mut data: NOTIFYICONDATAW = make_notify_icon_data(hwnd);
    data.uFlags = NIF_ICON;
    if Shell_NotifyIconW(NIM_DELETE, &mut data) == 0 {
        return Err(format!("Error deleting taskbar icon: {}", GetLastError()));
    }
    Ok(())
}

unsafe fn init_window(class_name: &[u16]) -> Result<WindowHandle, String> {
    register_class(class_name)?;
    let hwnd = create_window(class_name)?;
    let hmenu = create_popup_menu()?;
    create_notification_area_icon(hwnd)?;
    Ok(WindowHandle { hwnd, hmenu })
}

unsafe fn window_message_loop() {
    let mut msg: MaybeUninit<MSG> = MaybeUninit::uninit();
    let mut result = GetMessageW(msg.as_mut_ptr(), ptr::null_mut(), 0, 0);
    while result != 0 {
        if result == -1 {
            // TODO: destroy window
            // TODO: log error
            return;
        }
        TranslateMessage(msg.as_mut_ptr());
        DispatchMessageW(msg.as_mut_ptr());
        result = GetMessageW(msg.as_mut_ptr(), ptr::null_mut(), 0, 0);
    }
}

unsafe fn load_icon_from_file(file_name: &str) -> Result<HICON, String> {
    let hicon = LoadImageW(
        ptr::null_mut(),
        str_to_wchar_str(file_name).as_ptr(),
        IMAGE_ICON,
        0,
        0,
        LR_LOADFROMFILE,
    ) as HICON;
    if hicon.is_null() {
        return Err(format!("Error loading icon from file: {}", GetLastError()));
    }
    Ok(hicon)
}

unsafe fn load_icon_from_resource_by_name(name: &str) -> Result<HICON, String> {
    let hmodule = GetModuleHandleW(ptr::null_mut());
    if hmodule.is_null() {
        return Err(format!(
            "Error getting current module handle: {}",
            GetLastError()
        ));
    }
    let hicon = LoadImageW(
        hmodule,
        str_to_wchar_str(name).as_ptr(),
        IMAGE_ICON,
        0,
        0,
        0,
    ) as HICON;
    if hicon.is_null() {
        return Err(format!(
            "Error loading icon from resource: {}",
            GetLastError()
        ));
    }
    Ok(hicon)
}

unsafe fn load_icon_from_resource_by_ord(ord: u16) -> Result<HICON, String> {
    let hmodule = GetModuleHandleW(ptr::null_mut());
    if hmodule.is_null() {
        return Err(format!(
            "Error getting current module handle: {}",
            GetLastError()
        ));
    }
    let hicon = LoadImageW(hmodule, MAKEINTRESOURCEW(ord), IMAGE_ICON, 0, 0, 0) as HICON;
    if hicon.is_null() {
        return Err(format!(
            "Error loading icon from resource: {}",
            GetLastError()
        ));
    }
    Ok(hicon)
}

unsafe fn set_icon(hwnd: HWND, hicon: HICON) -> Result<(), String> {
    let mut data: NOTIFYICONDATAW = make_notify_icon_data(hwnd);
    data.uFlags = NIF_ICON;
    data.hIcon = hicon;
    if Shell_NotifyIconW(NIM_MODIFY, &mut data) == 0 {
        return Err(format!("Error setting taskbar icon: {}", GetLastError()));
    }
    Ok(())
}

unsafe fn set_tip(hwnd: HWND, tip: &str) -> Result<(), String> {
    let mut data: NOTIFYICONDATAW = make_notify_icon_data(hwnd);
    data.uFlags = NIF_TIP;
    copy_str_to_wchar_array(&mut data.szTip[..], tip);
    if Shell_NotifyIconW(NIM_MODIFY, &mut data) == 0 {
        return Err(format!(
            "Error setting taskbar icon tooltip: {}",
            GetLastError()
        ));
    }
    Ok(())
}

unsafe fn add_menu_item(hmenu: HMENU, id: u32, title: &str) -> Result<(), String> {
    let mut title = str_to_wchar_str(title);
    let mut item: MaybeUninit<MENUITEMINFOW> = MaybeUninit::uninit();
    let ptr = item.as_mut_ptr();
    addr_of_mut!((*ptr).cbSize).write(std::mem::size_of::<MENUITEMINFOW>() as UINT);
    addr_of_mut!((*ptr).fMask).write(MIIM_FTYPE | MIIM_STRING | MIIM_ID | MIIM_STATE);
    addr_of_mut!((*ptr).fType).write(MFT_STRING);
    addr_of_mut!((*ptr).fState).write(0);
    addr_of_mut!((*ptr).wID).write(id);
    addr_of_mut!((*ptr).dwTypeData).write(title.as_mut_ptr());
    if InsertMenuItemW(hmenu, id, 0, ptr) == 0 {
        return Err(format!("Error adding menu item: {}", GetLastError()));
    }
    Ok(())
}

unsafe fn add_menu_separator(hmenu: HMENU, id: u32) -> Result<(), String> {
    let mut item: MaybeUninit<MENUITEMINFOW> = MaybeUninit::uninit();
    let ptr = item.as_mut_ptr();
    addr_of_mut!((*ptr).cbSize).write(std::mem::size_of::<MENUITEMINFOW>() as UINT);
    addr_of_mut!((*ptr).fMask).write(MIIM_FTYPE | MIIM_ID);
    addr_of_mut!((*ptr).fType).write(MFT_SEPARATOR);
    addr_of_mut!((*ptr).wID).write(id);
    if InsertMenuItemW(hmenu, id, 0, ptr) == 0 {
        return Err(format!("Error adding menu separator: {}", GetLastError()));
    }
    Ok(())
}

unsafe fn show_balloon(hwnd: HWND, title: &str, body: &str) -> Result<(), String> {
    let mut data: NOTIFYICONDATAW = make_notify_icon_data(hwnd);
    data.uFlags = NIF_INFO;
    copy_str_to_wchar_array(&mut data.szInfo[..], body);
    *data.u.uTimeout_mut() = 30000;
    copy_str_to_wchar_array(&mut data.szInfoTitle[..], title);
    data.dwInfoFlags = NIIF_INFO;
    if Shell_NotifyIconW(NIM_MODIFY, &mut data) == 0 {
        return Err(format!(
            "Error setting taskbar icon balloon: {}",
            GetLastError()
        ));
    }
    Ok(())
}
