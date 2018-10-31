extern crate wna;
use wna::*;

fn main() {
    println!("hello!");
    let mut wna = Wna::new();
    wna
        .icon(Icon::File("resources/ico.ico".to_string()))
        .tip("Hello!")
        .menu_item(MenuItem::action("Quit".to_string(), quit))
        .menu_item(MenuItem::Separator)
        .menu_item(MenuItem::action("Show balloon".to_string(), move |wna| show_balloon(wna, "Balloon body")))
        .menu_item(MenuItem::Separator)
        .menu_item(MenuItem::action("Add item".to_string(), add_item));
    let mut wna = wna.build().unwrap();
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
    let _ = wna.add_menu_item(MenuItem::action("New item".to_string(), |_| println!("new item")));
}

fn show_balloon(wna: &mut Wna, message: &str) {
    println!("show balloon");
    let _ = wna.show_balloon("Balloon title", message, |_| println!("balloon clicked"));
}
