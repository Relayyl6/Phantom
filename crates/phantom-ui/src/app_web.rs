use eframe::WebRunner;
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub async fn start_web(canvas_id: &str) -> Result<(), wasm_bindgen::JsValue> {
    // Make sure panics are logged using `console.error`.
    console_error_panic_hook::set_once();
    tracing_wasm::set_as_global_default();

    let web_options = eframe::WebOptions::default();

    wasm_bindgen_futures::spawn_local(async {
        let runner = WebRunner::new();
        runner
            .start(
                canvas_id,
                web_options,
                Box::new(|cc| Ok(Box::new(crate::dashboard::DashboardApp::new(vec![])))),
            )
            .await
            .expect("failed to start eframe");
    });

    Ok(())
}
