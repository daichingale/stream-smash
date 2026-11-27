mod highlight;
mod session;
mod stream;
mod video;

use std::{env, error::Error, path::PathBuf, process::ExitCode};

use session::{CaptureSize, Input, Session};

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
        Some("devices") => {
            let names_only = match args.next().as_deref() {
                None => false,
                Some("--names") if args.len() == 0 => true,
                _ => return Err("Use devices or devices --names".into()),
            };
            session::list_devices(names_only)?;
        }
        Some(action @ ("record" | "preview")) => {
            let mut session = Session {
                preview_only: action == "preview",
                ..Session::default()
            };
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
                        session.size = parse_size(&size)?;
                        None
                    }
                    "--capture-size" => {
                        let size = value()?;
                        session.capture_size = if size == "auto" {
                            CaptureSize::Auto
                        } else {
                            CaptureSize::Fixed(parse_size(&size)?)
                        };
                        None
                    }
                    "--encoder" => {
                        session.encoder = value()?.parse()?;
                        None
                    }
                    "--quality" => {
                        session.quality = value()?.parse()?;
                        None
                    }
                    "--bitrate" => {
                        session.bitrate = value()?.parse()?;
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
                    "--highlights" => {
                        session.highlight_seconds = Some(value()?.parse()?);
                        None
                    }
                    "--audio-peaks" => {
                        session.audio_peaks = true;
                        None
                    }
                    "--audio-volume" => {
                        session.audio_volume = value()?.parse()?;
                        None
                    }
                    "--audio-meter" => {
                        session.audio_meter = true;
                        None
                    }
                    "--preview-image" => {
                        session.preview_image = Some(value()?.into());
                        None
                    }
                    "--preview-port" => {
                        session.preview_port = Some(value()?.parse()?);
                        None
                    }
                    "--preview-size" => {
                        session.preview_size = parse_size(&value()?)?;
                        None
                    }
                    "--preview-fps" => {
                        session.preview_fps = value()?.parse()?;
                        None
                    }
                    "--preview-quality" => {
                        session.preview_quality = value()?.parse()?;
                        None
                    }
                    "--overlay" => {
                        session.overlay = Some(value()?.into());
                        None
                    }
                    "--overlay-opacity" => {
                        session.overlay_opacity = value()?.parse()?;
                        None
                    }
                    "--camera-url" | "--camera-file" => {
                        if session.camera.is_some() {
                            return Err("Select only one secondary camera source".into());
                        }
                        let value = value()?;
                        session.camera = Some(if flag == "--camera-url" {
                            session::Camera::Network(value)
                        } else {
                            session::Camera::File(value.into())
                        });
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

fn parse_size(value: &str) -> Result<(u16, u16), Box<dyn Error>> {
    let (width, height) = value.split_once('x').ok_or("Use WIDTHxHEIGHT")?;
    Ok((width.parse()?, height.parse()?))
}

fn print_help() {
    println!(
        "Stream Smash\n\n\
         devices                        List Windows DirectShow inputs\n\
         record --test-pattern          Record a generated picture and tone\n\
         record --video-device NAME     Record a Windows capture device\n\
         record --input FILE            Record a media file at playback speed\n\n\
         preview --test-pattern --preview-image PATH   Preview without recording\n\
         Add --preview-image PATH to record for a live JPEG preview.\n\n\
         --preview-port PORT sends fixed-size BGRA frames to a loopback reader.\n\n\
         --camera-url URL adds a local MJPEG or RTSP inset. --camera-file FILE uses a local video.\n\
         --audio-volume 0-100 adjusts recording audio. --audio-meter prints level updates.\n\n\
         --preview-size WIDTHxHEIGHT, --preview-fps 1-60, --preview-quality 1-100\n\
         --overlay PNG overlays a full-canvas image; --overlay-opacity 0-100\n\n\
         Options: --audio-device NAME, --size WIDTHxHEIGHT, --fps N,\n\
                  --seconds N, --output-dir PATH, --stream, --highlights 10|30|60\n\
         --capture-size WIDTHxHEIGHT|auto (DirectShow only)\n\
         --encoder auto|cpu|amd|nvidia|intel, --quality fast|balanced|detail, --bitrate KBPS\n\
         --stream also sends RTMP(S) to STREAM_SMASH_RTMP_URL.\n\
         With highlights enabled, press h then Enter to save recent footage.\n\
         --audio-peaks also saves highlights when the audio suddenly gets louder.\n\
         Press q then Enter to stop and finalize the recording."
    );
}
