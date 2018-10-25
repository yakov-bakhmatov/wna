extern crate wna;
use wna::*;

fn main() {
    println!("hello!");
    let mut wna = Wna::new()
        .icon(Icon::File("resources/ico.ico".to_string()))
        .tip("Hello!")
        .menu_item(MenuItem::Action("Quit".to_string(), quit))
        .menu_item(MenuItem::Separator)
        .menu_item(MenuItem::Action("Show balloon".to_string(), show_balloon))
        .menu_item(MenuItem::Separator)
        .menu_item(MenuItem::Action("Add item".to_string(), add_item))
        .build().unwrap();
    ::std::thread::sleep(::std::time::Duration::from_millis(5000));
    let _ = wna.show_balloon("Greeting", "Hello, world!", |_| println!("greeting balloon clicked"));
    wna.join_event_loop();
    /*
    ::std::thread::sleep(::std::time::Duration::from_millis(15000));
    let _ = wna.close();
    */
}

fn quit(wna: &mut Wna) {
    println!("quit");
    let _ = wna.close();
    // ::std::process::exit(0);
}

fn add_item(wna: &mut Wna) {
    println!("add item");
    let _ = wna.add_menu_item(&MenuItem::Action("New item".to_string(), |_| println!("new item")));
}

fn show_balloon(wna: &mut Wna) {
    println!("show balloon");
    let _ = wna.show_balloon("Balloon title", "Balloon body", |_| println!("balloon clicked"));
}
