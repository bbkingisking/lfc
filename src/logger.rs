use anyhow::Result;
use anyhow::anyhow;
use ftail::Ftail;
use log::LevelFilter;
use log::info;
use std::fs;

const PKG_NAME: &str = env!("CARGO_PKG_NAME");

pub fn init_logger() -> Result<()> {
    let data_dir = dirs::data_dir().expect("Could not determine data directory");

    // Get final dir
    let logs_path = data_dir.join(PKG_NAME);

    // Get final dir + filename
    let logs_file = logs_path.join(format!("{}.log", PKG_NAME));

    // Try to create the final dir (this is idempotent so it's chill to run every time)
    match fs::create_dir_all(&logs_path) {
        Ok(_) => (),
        Err(e) => {
            return Err(anyhow!(
                "Could not create logs dir at {:#?}: {}",
                &logs_path,
                e
            ));
        }
    }

    // Initialize the logger
    match Ftail::new()
        .console(LevelFilter::Warn)
        .single_file(&logs_file, true, LevelFilter::Info)
        .init()
    {
        Ok(_) => {
            info!("Logger initialized.");
            Ok(())
        }
        Err(e) => return Err(anyhow!("Could not initialize logger: {}", e)),
    }
}
