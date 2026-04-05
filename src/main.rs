use std::process::ExitCode;

fn main() -> ExitCode {
    match tpm_rs::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            if error.should_print_stderr() {
                eprintln!("error: {error}");
            }
            error.exit_code()
        }
    }
}
