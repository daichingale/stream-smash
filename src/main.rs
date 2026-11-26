mod session;
mod stream;

use std::{env, error::Error, path::PathBuf, process::ExitCode};

use session::{Input, Session};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Stream Smash: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        None | Some("--help" | "-h") => print_help(),
        Some("devices") if args.len() == 0 => session::list_devices()?,
        Some("record") => {
            let mut session = Session::default();
            let mut input = None;
            let mut audio = None;
            while let Some(flag) = args.next() {
                let mut value = || {
                    args.next()
                        .ok_or_else(|| format!("{flag} requires a value"))
                };
                let selected = match flag.as_str() {
                    "--test-pattern" => Some(Input::TestPattern),
                    "--video-device" => Some(Input::DirectShow(value()?, None)),
                    "--input" => Some(Input::File(PathBuf::from(value()?))),
                    "--audio-device" => {
                        audio = Some(value()?);
                        None
                    }
                    "--output-dir" => {
                        session.output_dir = value()?.into();
                        None
                    }
                    "--size" => {
                        let size = value()?;
                        let (width, height) = size.split_once('x').ok_or("Use WIDTHxHEIGHT")?;
                        session.size = (width.parse()?, height.parse()?);
                        None
                    }
                    "--fps" => {
                        session.fps = value()?.parse()?;
                        None
                    }
                    "--seconds" => {
                        session.seconds = Some(value()?.parse()?);
                        None
                    }
                    "--stream" => {
                        session.stream = true;
                        None
                    }
                    _ => {
                        return Err(
                            format!("Unknown option {flag}. See stream-smash --help.").into()
                        );
                    }
                };
                if let Some(selected) = selected
                    && input.replace(selected).is_some()
                {
                    return Err("Choose exactly one video input".into());
                }
            }
            session.input = match (input, audio) {
                (Some(Input::DirectShow(video, _)), audio) => Input::DirectShow(video, audio),
                (Some(input), None) => input,
                (None, _) => return Err("Choose --video-device, --input, or --test-pattern".into()),
                _ => return Err("--audio-device requires --video-device".into()),
            };
            session.run()?;
        }
        _ => return Err("Unknown command. See stream-smash --help.".into()),
    }
    Ok(())
}

fn print_help() {
    println!(
        "Stream Smash\n\n\
         devices                        List Windows DirectShow inputs\n\
         record --test-pattern          Record a generated picture and tone\n\
         record --video-device NAME     Record a Windows capture device\n\
         record --input FILE            Record a media file at playback speed\n\n\
         Options: --audio-device NAME, --size WIDTHxHEIGHT, --fps N,\n\
                  --seconds N, --output-dir PATH, --stream\n\
         --stream also sends RTMP(S) to STREAM_SMASH_RTMP_URL.\n\
         Press q then Enter to stop and finalize the recording."
    );
}
