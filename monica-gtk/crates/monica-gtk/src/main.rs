mod passwords;
mod security;
mod state;
mod ui;
mod unlock;

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
