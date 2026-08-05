use lasso_zen_bre::{Config, run};

#[tokio::main]
async fn main() {
    let config = Config::from_env_and_args()
        .and_then(Config::prepare)
        .unwrap_or_else(|error| {
            eprintln!("configuration_error: {error}");
            std::process::exit(2);
        });

    if let Err(error) = run(config).await {
        eprintln!("service_error: {error}");
        std::process::exit(1);
    }
}
