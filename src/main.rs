use eframe;

mod gui;
mod proxy;
use gui::App;

fn main() {
    eframe::run_native(
        "Bridgeburner",
        eframe::NativeOptions::default(),
        Box::new(|cc| Box::new(App::new(cc)))
    );
}
