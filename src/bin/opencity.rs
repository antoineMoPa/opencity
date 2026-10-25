fn main() {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    match opencity::cli::run(args) {
        Some(Ok(())) => {}
        Some(Err(error)) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
        None => {
            eprintln!("{}", opencity::cli::HELP);
            std::process::exit(2);
        }
    }
}
