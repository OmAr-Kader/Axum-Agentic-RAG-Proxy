#![allow(dead_code)]

use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::{
    filter::filter_fn, fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer,
};

use crate::config::Config;

pub fn init_logging(config: &Config) {
    std::fs::create_dir_all(&config.log_dir).ok();

    let app_file_appender = RollingFileAppender::new(Rotation::DAILY, &config.log_dir, "app.log");
    let access_file_appender =
        RollingFileAppender::new(Rotation::DAILY, &config.log_dir, "access.log");

    let env_filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(&config.log_level));

    let stdout_layer = fmt::layer().with_writer(std::io::stdout);
    let app_file_layer = fmt::layer()
        .json()
        .with_writer(app_file_appender)
        .with_filter(filter_fn(|metadata| metadata.target() != "access"));
    let access_file_layer = fmt::layer()
        .json()
        .with_writer(access_file_appender)
        .with_filter(filter_fn(|metadata| metadata.target() == "access"));

    tracing_subscriber::registry()
        .with(env_filter)
        .with(stdout_layer)
        .with(app_file_layer)
        .with(access_file_layer)
        .init();
}
