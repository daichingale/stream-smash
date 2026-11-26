//! Versioned scene data. Output credentials are deliberately outside scene documents.
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const CANVAS_WIDTH: u32 = 1920;
pub const CANVAS_HEIGHT: u32 = 1080;
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LayerSource {
    Image { path: PathBuf },
    Browser { location: String },
    Timer,
    Waveform,
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Layer {
    pub name: String,
    pub enabled: bool,
    #[serde(default = "default_locked")]
    pub locked: bool,
    pub source: LayerSource,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub opacity: f32,
}
fn default_locked() -> bool {
    true
}
impl Layer {
    pub fn validate(&self) -> Result<(), String> {
        if self.name.len() > 200
            || self.width < 16
            || self.height < 16
            || self.width > 3840
            || self.height > 2160
            || !(-3840..=3840).contains(&self.x)
            || !(-2160..=2160).contains(&self.y)
            || !self.opacity.is_finite()
            || !(0.0..=1.0).contains(&self.opacity)
        {
            return Err("レイヤーのサイズ・位置・不透明度を確認してください".into());
        }
        if !self.enabled {
            return Ok(());
        }
        match &self.source {
            LayerSource::Image { path } => {
                if !path.is_file() {
                    return Err(format!("画像が見つかりません: {}", path.display()));
                }
                let reader = image::ImageReader::open(path)
                    .map_err(|_| "画像を開けません")?
                    .with_guessed_format()
                    .map_err(|_| "画像形式を判別できません")?;
                let (w, h) = reader
                    .into_dimensions()
                    .map_err(|_| "PNG / JPEG / WebP画像を選んでください")?;
                if w > 8192 || h > 8192 || w == 0 || h == 0 {
                    return Err("画像は8192×8192以内にしてください".into());
                }
            }
            LayerSource::Browser { location } => {
                browser_url(location)?;
            }
            LayerSource::Timer | LayerSource::Waveform => {}
        }
        Ok(())
    }
}

pub fn browser_url(location: &str) -> Result<String, String> {
    if location.starts_with("https://")
        || location.starts_with("http://")
        || location.starts_with("file://")
    {
        let url = url::Url::parse(location).map_err(|_| "ブラウザーのURLが不正です")?;
        if !url.username().is_empty() || url.password().is_some() {
            return Err("ユーザー情報を埋め込んだURLは使えません".into());
        }
        if url.scheme() == "file"
            && !url
                .to_file_path()
                .map_err(|_| "ローカルHTMLのパスが不正です")?
                .is_file()
        {
            return Err("HTMLファイルが見つかりません".into());
        }
        Ok(url.to_string())
    } else {
        let path = std::fs::canonicalize(location)
            .map_err(|_| "HTMLファイルまたはhttp(s) URLを指定してください")?;
        url::Url::from_file_path(path)
            .map(|u| u.to_string())
            .map_err(|_| "HTMLパスをURLに変換できません".into())
    }
}
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct Scene {
    pub version: u32,
    pub layers: Vec<Layer>,
}
impl Default for Scene {
    fn default() -> Self {
        Self {
            version: 1,
            layers: Vec::new(),
        }
    }
}
impl Scene {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1 || self.layers.len() > 8 {
            return Err("シーンはversion 1、最大8レイヤーに対応しています".into());
        }
        for layer in &self.layers {
            layer.validate()?;
        }
        Ok(())
    }
    pub fn load(path: &Path) -> Result<Self, String> {
        if std::fs::metadata(path).map_err(|e| e.to_string())?.len() > 1024 * 1024 {
            return Err("シーンファイルが大きすぎます".into());
        }
        let mut scene: Self =
            serde_json::from_slice(&std::fs::read(path).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        // Portable scene documents resolve relative assets beside the scene file.
        for layer in &mut scene.layers {
            match &mut layer.source {
                LayerSource::Image { path: asset } if asset.is_relative() => {
                    *asset = path.parent().unwrap_or(Path::new(".")).join(&asset)
                }
                LayerSource::Browser { location }
                    if !location.contains("://") && Path::new(location).is_relative() =>
                {
                    *location = path
                        .parent()
                        .unwrap_or(Path::new("."))
                        .join(&location)
                        .to_string_lossy()
                        .into_owned()
                }
                _ => {}
            }
        }
        scene.validate()?;
        Ok(scene)
    }
    pub fn save(&self, path: &Path) -> Result<(), String> {
        self.validate()?;
        std::fs::write(
            path,
            serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())
    }
}

/// Every output branches AFTER composition. Preview-only decoration is never used.
pub(crate) struct GraphInputs<'a> {
    pub program: Option<usize>,
    pub audio: Option<&'a str>,
    pub timer: Option<(&'a Path, &'a Path)>,
}
fn filter_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace('\\', "/")
        .replace(':', "\\:")
        .replace(char::from(39), "'\\''")
}
pub(crate) fn filter_graph(
    layers: &[(usize, &Layer)],
    output: (u32, u32),
    preview: (u32, u32, u32),
    record: bool,
    stream: bool,
    replay: bool,
    inputs: GraphInputs<'_>,
) -> String {
    let (width, height) = output;
    let mut graph = if layers.is_empty() {
        "[0:v]setsar=1[base0];".to_string()
    } else {
        format!("[0:v]scale={CANVAS_WIDTH}:{CANVAS_HEIGHT}:force_original_aspect_ratio=decrease:flags=lanczos,pad={CANVAS_WIDTH}:{CANVAS_HEIGHT}:(ow-iw)/2:(oh-ih)/2,setsar=1,format=rgba[base0];")
    };
    for (n, (input, layer)) in layers.iter().enumerate() {
        if matches!(layer.source, LayerSource::Timer) {
            let (path, font) = inputs.timer.expect("validated timer");
            let size = (layer.height * 4 / 5).min(layer.width / 7).max(8);
            graph.push_str(&format!("[base{n}]drawtext=textfile='{}':reload=1:fontfile='{}':fontsize={size}:fontcolor=white:borderw=2:bordercolor=black:x={}:y={}:alpha={:.4}[base{}];",filter_path(path),filter_path(font),layer.x,layer.y,layer.opacity,n+1));
            continue;
        }

        let source = if matches!(layer.source, LayerSource::Waveform) {
            format!("[{}]aformat=channel_layouts=mono,showwaves=s={}x{}:mode=line:rate=15:colors=0x36d9ff:draw=full:scale=sqrt,format=rgba,colorkey=0x000000:0.01:0.0",inputs.audio.expect("validated audio"),layer.width,layer.height)
        } else {
            format!("[{input}:v]format=rgba")
        };
        let shortest = if matches!(layer.source, LayerSource::Waveform) {
            ":shortest=1"
        } else {
            ""
        };
        graph.push_str(&format!("{source},scale={}:{}:force_original_aspect_ratio=decrease:flags=lanczos,pad={}:{}:(ow-iw)/2:(oh-ih)/2:color=black@0,setsar=1,colorchannelmixer=aa={:.4}[layer{n}];[base{n}][layer{n}]overlay=x={}:y={}:eof_action=repeat:repeatlast=1:format=auto:alpha=straight{shortest}[base{}];",layer.width,layer.height,layer.width,layer.height,layer.opacity,layer.x,layer.y,n+1));
    }
    let mut base = format!("base{}", layers.len());
    if let Some(input) = inputs.program {
        if replay {
            graph.push_str(&format!("[{base}]split=2[liveinsert][b0];"));
            base = "liveinsert".into();
        }
        graph.push_str(&format!("[{input}:v]scale={width}:{height}:flags=lanczos,setsar=1[insert];[{base}]scale={width}:{height}:flags=lanczos,setsar=1[liveprogram];[liveprogram][insert]overlay=eof_action=pass:format=auto[program];"));
        base = "program".into();
    }
    let mut branches = vec!["p0"];
    if record {
        branches.push("r0");
    }
    if stream {
        branches.push("s0");
    }
    if replay && inputs.program.is_none() {
        branches.push("b0");
    }
    if branches.len() == 1 {
        graph.push_str(&format!("[{base}]null[p0];"));
    } else {
        graph.push_str(&format!(
            "[{base}]split={}[{}];",
            branches.len(),
            branches.join("][")
        ));
    }
    let (pw, ph, pfps) = preview;
    graph.push_str(&format!(
        "[p0]select='isnan(prev_selected_t)+gte(t-prev_selected_t,{:.6})',scale={pw}:{ph}:flags=lanczos,format=rgba[v_preview]",1.0/f64::from(pfps)-0.0001
    ));
    for (enabled, name) in [(record, "record"), (stream, "stream"), (replay, "buffer")] {
        if enabled {
            graph.push_str(&format!(
                ";[{}0]scale={width}:{height}:flags=lanczos,format=yuv420p[v_{name}]",
                &name[..1]
            ));
        }
    }
    graph
}

#[cfg(test)]
mod tests {
    use super::*;
    fn timer() -> Layer {
        Layer {
            name: "RTA".into(),
            enabled: true,
            locked: true,
            source: LayerSource::Timer,
            x: 20,
            y: 20,
            width: 600,
            height: 100,
            opacity: 1.0,
        }
    }
    #[test]
    fn old_scenes_start_locked_and_unlock_is_preserved() {
        let mut value = serde_json::to_value(timer()).unwrap();
        value.as_object_mut().unwrap().remove("locked");
        let mut layer: Layer = serde_json::from_value(value).unwrap();
        assert!(layer.locked);
        layer.locked = false;
        let loaded: Layer = serde_json::from_str(&serde_json::to_string(&layer).unwrap()).unwrap();
        assert!(!loaded.locked);
    }
    #[test]
    fn versioned_scenes_round_trip_without_output_credentials() {
        let scene = Scene {
            version: 1,
            layers: vec![timer()],
        };
        scene.validate().unwrap();
        let json = serde_json::to_string(&scene).unwrap();
        assert_eq!(scene, serde_json::from_str(&json).unwrap());
        assert!(!json.contains("key"));
        assert!(Scene {
            version: 99,
            layers: vec![]
        }
        .validate()
        .is_err());
        assert!(Scene {
            version: 1,
            layers: vec![timer(); 9]
        }
        .validate()
        .is_err());
    }
    #[test]
    fn invalid_urls_and_coordinates_are_rejected() {
        for url in ["javascript:alert(1)", "https://user:password@example.com/a"] {
            assert!(browser_url(url).is_err());
        }
        let mut layer = timer();
        layer.opacity = f32::NAN;
        assert!(layer.validate().is_err());
        layer.opacity = 1.0;
        layer.x = 99999;
        assert!(layer.validate().is_err());
    }
}
