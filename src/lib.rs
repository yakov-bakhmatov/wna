#[macro_use]
extern crate error_chain;
extern crate winapi;

mod window;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::*;
use std::thread;

error_chain! {

}

pub type Action = fn(&mut Wna) -> ();

pub enum Icon {
    File(String),
    ResourceByName(String),
    ResourceByOrd(u16),
}

pub enum MenuItem {
    Action(String, Action),
    Separator,
}

pub enum Event {
    Menu(u32),
    Balloon,
    Quit,
}

pub struct Wna {
    repr: Arc<Mutex<Repr>>,
    thread: Option<thread::JoinHandle<()>>,
}

#[derive(Default)]
pub struct WnaBuilder {

    window_class: Option<&'static str>,
    icon: Option<Icon>,
    tip: Option<String>,
    menu_items: Vec<MenuItem>,

}

impl Wna {

    pub fn new() -> WnaBuilder {
        WnaBuilder::default()
    }

    pub fn set_icon(&mut self, icon: &Icon) -> Result<()> {
        let mut lock = self.repr.lock().unwrap();
        lock.set_icon(icon)
    }

    pub fn set_tip(&mut self, tip: &str) -> Result<()> {
        let mut lock = self.repr.lock().unwrap();
        lock.set_tip(tip)
    }

    pub fn add_menu_item(&mut self, item: &MenuItem) -> Result<()> {
        let mut lock = self.repr.lock().unwrap();
        lock.add_menu_item(item)
    }

    pub fn show_balloon(&mut self, title: &str, body: &str, action: Action) -> Result<()> {
        let mut lock = self.repr.lock().unwrap();
        lock.show_balloon(title, body, action)
    }

    pub fn close(&mut self) -> Result<()> {
        let mut lock = self.repr.lock().unwrap();
        lock.close()
    }

    pub fn join_event_loop(self) {
        if let Some(thread) = self.thread {
            let _ = thread.join();
        }
    }

}

impl Clone for Wna {
    fn clone(&self) -> Self {
        Wna {
            repr: Arc::clone(&self.repr),
            thread: None,
        }
    }
}

impl WnaBuilder {

    pub fn window_class(&mut self, class: &'static str) -> &mut Self {
        self.window_class = Some(class);
        self
    }

    pub fn icon(&mut self, icon: Icon) -> &mut Self {
        self.icon = Some(icon);
        self
    }

    pub fn tip(&mut self, tip: &str) -> &mut Self {
        self.tip = Some(tip.to_string());
        self
    }

    pub fn menu_item(&mut self, item: MenuItem) -> &mut Self {
        self.menu_items.push(item);
        self
    }

    pub fn build(&mut self) -> Result<Wna> {
        let (sender, reciever) = channel();
        let window_class = self.window_class.unwrap_or("wna_window_class");
        let window = window::Window::create(window_class, sender.clone())?;
        let mut repr = Repr {
            window: window,
            last_menu_id: 0,
            actions: HashMap::new(),
            balloon_action: None,
            event_sender: sender,
        };
        if let Some(ref icon) = self.icon {
            repr.set_icon(icon)?;
        }
        if let Some(ref tip) = self.tip {
            repr.set_tip(tip)?;
        }
        for item in self.menu_items.iter() {
            repr.add_menu_item(item)?;
        }
        let repr = Arc::new(Mutex::new(repr));
        let thread = start_event_loop(reciever, Arc::clone(&repr));
        Ok(Wna {
            repr: repr,
            thread: Some(thread),
        })
    }

}

struct Repr {
    window: window::Window,
    last_menu_id: u32,
    actions: HashMap<u32, Action>,
    balloon_action: Option<Action>,
    event_sender: Sender<Event>,
}

impl Repr {

    fn next_menu_id(&mut self) -> u32 {
        let id = self.last_menu_id;
        self.last_menu_id += 1;
        id
    }

    pub fn set_icon(&mut self, icon: &Icon) -> Result<()> {
        self.window.set_icon(icon)
    }

    pub fn set_tip(&mut self, tip: &str) -> Result<()> {
        self.window.set_tip(tip)
    }

    pub fn add_menu_item(&mut self, item: &MenuItem) -> Result<()> {
        match item {
            MenuItem::Action(ref title, ref action) => {
                let id = self.next_menu_id();
                self.window.add_menu_item(id, title)?;
                self.actions.insert(id, *action);
                Ok(())
            },
            MenuItem::Separator => {
                let id = self.next_menu_id();
                self.window.add_menu_separator(id)
            }
        }
    }

    pub fn show_balloon(&mut self, title: &str, body: &str, action: Action) -> Result<()> {
        self.window.show_balloon(title, body)?;
        self.balloon_action = Some(action);
        Ok(())
    }

    pub fn close(&mut self) -> Result<()> {
        self.window.close();
        let _ = self.event_sender.send(Event::Quit);
        Ok(())
    }

}

impl Drop for Repr {
    fn drop(&mut self) {
        let _ = self.close();
    }
}

fn start_event_loop(receiver: Receiver<Event>, repr: Arc<Mutex<Repr>>) -> thread::JoinHandle<()> {
    thread::Builder::new().name("wna-event-loop".into()).spawn(move || {
        loop {
            match receiver.recv() {
                Ok(event) => match event {
                    Event::Menu(id) => {
                        let action = {
                            let mut repr = repr.lock().unwrap();
                            repr.actions.get(&id).map(|f| *f)
                        };
                        if let Some(action) = action {
                            let mut wna = Wna {
                                repr: Arc::clone(&repr),
                                thread: None,
                            };
                            action(&mut wna);
                        }
                    }
                    Event::Balloon => {
                        let action = {
                            let mut repr = repr.lock().unwrap();
                            repr.balloon_action.take()
                        };
                        if let Some(action) = action {
                            let mut wna = Wna {
                                repr: Arc::clone(&repr),
                                thread: None,
                            };
                            action(&mut wna);
                        }
                    }
                    Event::Quit => {
                        return;
                    }
                },
                Err(_) => { 
                    return;
                }
            }
        }
    }).unwrap()
}
