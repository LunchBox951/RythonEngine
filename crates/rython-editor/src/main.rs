use rython_editor::app::EditorApp;

fn main() -> eframe::Result<()> {
    env_logger::init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1600.0, 900.0])
            .with_title("RythonEditor"),
        renderer: eframe::Renderer::Wgpu,
        ..Default::default()
    };

    eframe::run_native(
        "RythonEditor",
        options,
        Box::new(|cc| Ok(Box::new(EditorApp::new(cc)))),
    )
}
