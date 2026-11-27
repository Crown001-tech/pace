#[tokio::main]
async fn main() {
    pace_render::worker::run_render_worker_loop().await;
}
