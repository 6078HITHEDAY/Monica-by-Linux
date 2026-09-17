mod backup_sync;
mod dialogs;
mod generator;
mod lifecycle;
mod notes;
mod otp;
mod pages;
mod passwords;
mod prefs;
mod security;
mod state;
mod timeline;
mod transfer;
mod ui;
mod unlock;
mod wallet;
mod widgets;
mod workbench;

use monica_vault::self_test;

fn main() {
    let mut args = std::env::args().skip(1);
    if args.any(|argument| argument == "--self-test") {
        match self_test() {
            Ok(summary) => {
                println!("{summary}");
            }
            Err(error) => {
                eprintln!("monica-gtk self-test failed: {error}");
                std::process::exit(1);
            }
        }
        return;
    }

    ui::run();
}
