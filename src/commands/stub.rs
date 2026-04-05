use crate::error::{AppError, Result};

pub fn run(command: &'static str) -> Result<()> {
    Err(AppError::NotImplemented { command })
}
