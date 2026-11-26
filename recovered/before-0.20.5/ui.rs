use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::{cell::RefCell, path::PathBuf, rc::Rc, time::Duration};
use streamsmash_core::{VideoFormat, VideoSource};
use streamsmash_media::{recorder::*, TestPattern};

slint::slint! {
    import { Palette } from "std-widgets.slint";
    import { Button, VerticalBox, HorizontalBox, ComboBox, LineEdit, SpinBox, ScrollView, CheckBox } from "std-widgets.slint";
    export component OutputWindow inherits Window {
        title: "Stream Smash — 配信映像（リプレイ含む）";preferred-width: 1280px;preferred-height: 720px;background: black;
        in property <image> frame;
        Image {source: root.frame;image-fit: contain;image-rendering: smooth;width: parent.width;height: parent.height;}
    }
    export component MainWindow inherits Window {
        title: "Stream Smash 0.20.4";
        default-font-size: 14px;
        // Match all standard controls to the studio background, regardless of OS theme.
        init => { Palette.color-scheme = ColorScheme.dark; }
        preferred-width: 1280px; preferred-height: 860px;
        min-width: 1040px; min-height: 720px;
        background: #111827;
        in property <image> preview;
        in property <string> performance-note: "測定中";
        in property <[string]> sources;
        in property <[string]> audio-sources;
        in-out property <int> source-index: 0;
        in-out property <int> audio-index: 0;
        in-out property <int> quality-index: 0;
        in-out property <int> preview-index: 0;
        in-out property <bool> expanded: false;
        in-out property <bool> settings-open: false;
        in-out property <bool> show-timer: false;
        public function show-settings(page: int) {
            root.studio-page = page;
            if page == 0 {root.basic-tab=0;}
            root.settings-open = true;
            root.expanded = false;
        }
        public function show-workspace() {
            root.settings-open = false;
            root.expanded = false;
        }

        in-out property <int> encoder-index: 0;
        in property <bool> encoder-probing: false;
        in property <string> encoder-note: "CPU負荷が高いときは、使えるGPU圧縮を確認できます。";
        callback probe-encoder();
        in-out property <int> audio-format-index: 0;
        in-out property <int> audio-offset-ms: 0;
        in-out property <bool> show-audio-settings: false;
        in-out property <int> basic-tab: 0;
        in-out property <int> custom-width: 1280;
        in-out property <int> custom-height: 720;
        in-out property <int> custom-fps: 30;
        in-out property <int> audio-gain: 100;
        in-out property <bool> primary-muted: false;
        in-out property <bool> microphone-enabled: false;
        in property <[string]> microphones: ["Windowsの既定マイク"];
        in-out property <int> microphone-index: 0;
        in-out property <int> microphone-gain: 100;
        in-out property <bool> microphone-muted: false;
        in-out property <int> microphone-offset-ms: 0;
        in-out property <bool> audio-limiter: false;
        in-out property <int> audio-kbps: 0;
        in-out property <int> record-crf: 21;
        in-out property <int> gpu-record-kbps: 12000;
        in property <string> settings-note: "迷ったら、画質と音の入力先を選んでプレビューを確認しましょう。";
        callback settings-preset(int);
        callback settings-defaults();
        callback save-preferences() -> bool;
        in-out property <int> monitor: 0;
        in-out property <string> folder;
        in-out property <string> replay-folder;
        in property <bool> replay-folder-picking: false;
        callback choose-replay-folder();
        callback choose-record-folder();
        in property <bool> record-folder-picking: false;
        callback apply-replay-folder();
        callback open-replay-folder();
        in property <string> status: "入力を選び、プレビューまたは録画を開始してください。";
        in property <string> elapsed: "00:00";
        in property <bool> active: false;
        in property <bool> recording: false;
        in property <bool> busy: false;
        in property <bool> has-image: false;
        in-out property <int> studio-page: 0;
        in-out property <[string]> layers;
        in-out property <int> layer-index: -1;
        in-out property <int> layer-kind: 0;
        in-out property <string> layer-name: "オーバーレイ";
        in-out property <string> layer-location;
        in-out property <int> layer-x: 0;
        in-out property <int> layer-y: 0;
        in-out property <int> layer-width: 600;
        in-out property <int> layer-height: 150;
        in-out property <int> layer-opacity: 100;
        in-out property <bool> layer-enabled: true;
        in-out property <string> stream-server: "rtmps://a.rtmps.youtube.com:443/live2";
        in-out property <string> stream-key;
        in-out property <int> bitrate: 6000;
        in-out property <bool> stream-record: false;
        in-out property <bool> replay-enabled: false;
        in-out property <int> replay-seconds: 30;
        in property <bool> streaming: false;
        in property <bool> buffering: false;
        in property <string> camera-status: "仮想カメラ: OFF";
        in property <string> race-time: "00:00:00.00";
        callback layer-select(int);
        callback layer-add();
        callback layer-apply();
        callback layer-remove();
        callback layer-duplicate();
        callback layer-lock(bool);
        callback layer-dragged();
        in-out property <bool> layer-locked: true;
        callback editor-history(bool);
        in property <bool> editor-can-undo: false;
        in property <bool> editor-can-redo: false;
        callback layer-move(int);
        callback scene-save();
        callback scene-load();
        callback plugin-action(int);
        callback phone-action(int);
        in property <bool> phone-enabled: false;
        in-out property <string> phone-ip;
        in-out property <int> phone-port: 8747;
        in property <string> phone-status: "OFF：同じWi-Fi内のスマホ・別PCから接続できます";
        in property <string> phone-url;
        in property <string> phone-code;
        in property <string> phone-fingerprint;
        in property <string> phone-pending;
        in-out property <bool> phone-allow-control: false;
        in property <[string]> phone-devices;
        in-out property <int> phone-device-index: 0;
        in property <image> phone-qr;
        in property <[string]> plugin-names;
        in-out property <int> plugin-index: 0;
        in property <bool> plugin-candidate: false;
        in property <bool> plugin-busy: false;
        in property <string> plugin-details: "画像・HTML形式のStream Smashプラグインを導入できます。OBS用DLLは未対応です。";
        in-out property <bool> virtual-camera: false;
        in-out property <bool> virtual-camera-mirror: false;
        in-out property <bool> replaylab-enabled: false;
        in property <string> replaylab-status: "SmashReplayLab連携: OFF";
        callback toggle-replaylab(bool);
        callback launch-replaylab();
        callback auto-action(int);
        in-out property <int> auto-page: 0;
        in-out property <bool> auto-details: false;
        in property <bool> auto-busy: false;
        in property <bool> auto-monitoring: false;
        in-out property <bool> auto-dry: false;
        in-out property <bool> auto-repeat: false;
        in-out property <int> auto-detection: 2;
        in-out property <int> auto-training: 0;
        in-out property <int> highlight-seconds: 10;
        in-out property <bool> highlight-autoplay: true;
        in-out property <int> highlight-delay: 5;
        in-out property <int> highlight-post-roll: 1;
        in property <int> highlight-count: 0;
        in property <bool> highlight-playing: false;
        in-out property <bool> replay-tv-overlay: true;
        in-out property <int> auto-x: 130;
        in-out property <int> auto-y: 50;
        in-out property <int> auto-width: 380;
        in-out property <int> auto-height: 195;
        in property <string> auto-status: "自動認識は停止中です";
        in property <string> auto-score: "一致度: --";
        in property <string> auto-learning: "学習データを確認しています…";
        callback install-camera();
        callback diagnose-camera();
        callback show-output();
        callback start-stream();
        callback save-replay();
        callback timer-action(int);
        callback start-preview();
        callback start-recording();
        callback stop();
        callback refresh();
        callback open-folder();
        MenuBar {
            Menu { title: "ファイル";
                MenuItem {title: "シーンを開く…";shortcut: @keys(Control + O);enabled: !root.recording && !root.streaming && !root.busy;activated => {root.scene-load();}}
                MenuItem {title: "シーンを保存…";shortcut: @keys(Control + S);activated => {root.scene-save();}}
                MenuSeparator {}
                MenuItem {title: "録画フォルダーを開く";activated => {root.open-folder();}}
                MenuItem {title: "リプレイフォルダーを開く";activated => {root.open-replay-folder();}}
            }
            Menu {title: "編集";
                MenuItem {title: "元に戻す";enabled: root.editor-can-undo && !root.recording && !root.streaming && !root.busy;activated => {root.editor-history(false);}}
                MenuItem {title: "やり直す";enabled: root.editor-can-redo && !root.recording && !root.streaming && !root.busy;activated => {root.editor-history(true);}}
                MenuSeparator {}
                MenuItem {title: "レイヤーの追加・設定…";activated => {root.show-settings(1);}}
                MenuItem {title: "選択レイヤーを複製";enabled: root.layer-index >= 0 && !root.recording && !root.streaming && !root.busy;activated => {root.layer-duplicate();}}
            }
            Menu {title: "表示";
                MenuItem {title: "標準の作業画面に戻す";activated => {root.show-workspace();}}
                MenuItem {title: root.expanded ? "操作パネルを表示" : "プレビューを大きく表示";activated => {root.expanded = !root.expanded;}}
                MenuItem {title: root.show-timer ? "RTA操作を隠す" : "RTA操作を表示";activated => {root.show-timer = !root.show-timer;}}
                MenuItem {title: "共有用の映像ウィンドウ";activated => {root.show-output();}}
            }
            Menu {title: "設定";
                MenuItem {title: "映像・音声・保存先…";activated => {root.show-settings(0);}}
                MenuItem {title: "映像の詳細…";activated => {root.show-settings(0);root.basic-tab=1;}}
                MenuItem {title: "音声の詳細…";activated => {root.show-settings(0);root.basic-tab=2;}}
                MenuSeparator {}
                MenuItem {title: "YouTube・配信先…";activated => {root.show-settings(2);}}
                MenuItem {title: "リプレイ・画像認識…";activated => {root.show-settings(3);}}
                MenuItem {title: "Discord・仮想カメラ…";activated => {root.show-settings(7);}}
            }
            Menu {title: "ツール";
                MenuItem {title: "プラグイン…";activated => {root.show-settings(5);}}
                MenuItem {title: "スマホ・別PCリモコン…";activated => {root.show-settings(6);}}
                MenuItem {title: "SmashReplayLab連携…";activated => {root.show-settings(4);}}
            }
            Menu {title: "ヘルプ";
                MenuItem {title: "はじめて使う方へ";activated => {root.show-settings(8);}}
                MenuItem {title: "Stream Smashについて";activated => {root.show-settings(8);}}
            }
        }
        VerticalBox {
            padding: 12px; spacing: 8px;
            HorizontalBox {
                height: 32px;
                Text {text: "Stream Smash";color: #f8fafc;font-size: 20px;font-weight: 700;vertical-alignment: center;}
                if root.active: Text {text: root.performance-note;color: #94a3b8;font-size: 12px;vertical-alignment: center;}
                Text {text: root.highlight-playing ? "● リプレイを送出中" : root.streaming && root.recording ? "● 配信・録画中  " + root.elapsed : root.streaming ? "● 配信送信中  " + root.elapsed : root.recording ? "● 録画中  " + root.elapsed : root.active ? "● プレビュー中" : "準備中";color: root.recording || root.streaming ? #fda4af : #67e8f9;horizontal-alignment: right;vertical-alignment: center;horizontal-stretch: 1;}
                Button {text: root.expanded ? "作業画面に戻る" : "プレビューを拡大";clicked => {if root.expanded {root.show-workspace();} else {root.expanded=true;}}}
            }
            // Keep the six primary controls in stable positions on every page.
            HorizontalBox {
                height: 36px;padding: 0px;spacing: 8px;
                Button {width: 164px;primary: !root.active;text: root.active ? "プレビュー更新" : "プレビュー開始";enabled: !root.recording && !root.streaming && !root.busy && !root.encoder-probing;clicked => {root.start-preview();}}
                Button {width: 120px;text: "プレビュー停止";enabled: root.active && !root.recording && !root.streaming && !root.busy;clicked => {root.stop();}}
                Button {width: 120px;primary: root.active && !root.recording && !root.streaming;text: "● 録画開始";enabled: !root.recording && !root.streaming && !root.busy && !root.encoder-probing;clicked => {root.start-recording();}}
                Button {width: 120px;text: "録画停止";enabled: root.recording && !root.streaming && !root.busy;clicked => {root.stop();}}
                Button {width: 120px;text: "配信開始";enabled: !root.recording && !root.streaming && !root.busy && !root.encoder-probing;clicked => {if root.stream-key == "" {root.show-settings(2);} else {root.start-stream();}}}
                Button {width: 120px;text: "配信停止";enabled: root.streaming && !root.busy;clicked => {root.stop();}}
                if root.busy: Button {width: 120px;text: "開始を中止";clicked => {root.stop();}}
                Rectangle {horizontal-stretch: 1;}
            }
            if !root.expanded: HorizontalBox {
                Text { text: "映像入力"; color: #e2e8f0; vertical-alignment: center; }
                ComboBox { model: root.sources; current-index <=> root.source-index; enabled: !root.recording && !root.streaming && !root.busy; horizontal-stretch: 1; }
                Text { text: "画面番号"; color: #e2e8f0; vertical-alignment: center; }
                SpinBox { width: 100px;horizontal-stretch: 0;value <=> root.monitor; minimum: 0; maximum: 15; enabled: root.source-index == 0 && !root.recording && !root.streaming && !root.busy; }
                Button { text: "デバイス更新"; enabled: !root.active && !root.busy; clicked => { root.refresh(); } }
            }
            HorizontalBox {
                spacing: 10px;vertical-stretch: 1;min-height: root.settings-open && !root.expanded ? (root.buffering || root.auto-monitoring || root.highlight-playing || root.studio-page == 3 ? (root.auto-monitoring ? 286px : 242px) : 150px) : 380px;
            VerticalBox {padding: 0px;spacing: 8px;horizontal-stretch: 1;
            preview-area := Rectangle {
                background: #030712;clip: true; vertical-stretch: 1; min-height: root.settings-open ? 130px : 180px;
                private property <length> content-width: min(self.width, self.height * 16 / 9);
                private property <length> content-height: self.content-width * 9 / 16;
                Image { source: root.preview; image-fit: contain; image-rendering: smooth; width: parent.width; height: parent.height; visible: root.has-image; }
                if root.studio-page == 3 && root.auto-page == 1 && root.has-image && !root.expanded: Rectangle {
                    x: (parent.width - preview-area.content-width) / 2 + root.auto-x * preview-area.content-width / 640;
                    y: (parent.height - preview-area.content-height) / 2 + root.auto-y * preview-area.content-height / 360;
                    width: root.auto-width * preview-area.content-width / 640;
                    height: root.auto-height * preview-area.content-height / 360;
                    background: transparent;border-color: #facc15;border-width: 2px;
                }
                if root.studio-page == 1 && root.layer-index >= 0 && root.has-image: Rectangle {
                    x: (parent.width - preview-area.content-width)/2 + root.layer-x*preview-area.content-width/1920;
                    y: (parent.height - preview-area.content-height)/2 + root.layer-y*preview-area.content-height/1080;
                    width: root.layer-width*preview-area.content-width/1920;
                    height: root.layer-height*preview-area.content-height/1080;
                    background: transparent;border-width: 2px;border-color: root.layer-locked ? #94a3b8 : #36d9ff;
                }
                drag-area := TouchArea {
                    width: parent.width;height: parent.height;
                    enabled: root.studio-page == 1 && root.layer-index >= 0 && root.layer-enabled && !root.layer-locked && root.has-image && !root.recording && !root.streaming && !root.busy && !root.auto-monitoring;
                    private property <int> start-x;
                    private property <int> start-y;
                    private property <bool> dragging: false;
                    pointer-event(event) => {
                        if event.button == PointerEventButton.left && event.kind == PointerEventKind.down {
                            self.start-x=root.layer-x;self.start-y=root.layer-y;
                            self.dragging=self.mouse-x >= (self.width - preview-area.content-width)/2 + root.layer-x*preview-area.content-width/1920 && self.mouse-x <= (self.width - preview-area.content-width)/2 + (root.layer-x+root.layer-width)*preview-area.content-width/1920 && self.mouse-y >= (self.height - preview-area.content-height)/2 + root.layer-y*preview-area.content-height/1080 && self.mouse-y <= (self.height - preview-area.content-height)/2 + (root.layer-y+root.layer-height)*preview-area.content-height/1080;
                        }
                        if event.kind == PointerEventKind.up && self.dragging {
                            self.dragging=false;root.layer-dragged();
                        }
                        if event.kind == PointerEventKind.cancel && self.dragging {
                            self.dragging=false;root.layer-x=self.start-x;root.layer-y=self.start-y;
                        }
                    }
                    moved => {
                        if self.pressed && self.dragging {
                            root.layer-x=round(clamp(self.start-x+(self.mouse-x - self.pressed-x)*1920/preview-area.content-width,-3840,3840));
                            root.layer-y=round(clamp(self.start-y+(self.mouse-y - self.pressed-y)*1080/preview-area.content-height,-2160,2160));
                        }
                    }
                }
                Text { text: "配信画面のプレビュー\n上の映像入力を選び、上の「プレビュー開始」を押してください。"; color: #64748b; horizontal-alignment: center; vertical-alignment: center; visible: !root.has-image; }
            }
            if root.buffering || root.auto-monitoring || root.highlight-playing || (root.settings-open && root.studio-page == 3): Rectangle {
                min-height: root.auto-monitoring ? 148px : 104px;vertical-stretch: 0;
                background: root.highlight-playing ? #312e81 : #172b3b;
                border-color: root.highlight-playing ? #a5b4fc : #36d9ff;border-width: 1px;border-radius: 6px;
                VerticalBox {padding: 10px;spacing: 6px;
                    HorizontalBox {
                        Text {text: root.highlight-playing ? "● REPLAY" : root.auto-monitoring ? "● 自動リプレイ動作中" : root.buffering ? "● リプレイ蓄積中（認識停止）" : "自動リプレイ";color: #f8fafc;font-weight: 700;}
                        Text {text: "保存済み " + root.highlight-count + " 本";color: #67e8f9;horizontal-alignment: right;}
                        Button {text: "保存先を開く";clicked => {root.open-replay-folder();}}
                        Button {text: "リプレイ全停止";enabled: root.buffering && !root.recording && !root.streaming && !root.busy;clicked => {root.auto-action(17);}}
                    }
                    Text {text: root.auto-status;color: #f1f5f9;wrap: word-wrap;}
                    if root.auto-monitoring: HorizontalBox {
                        Button {text: "今すぐ再生";enabled: root.active && root.highlight-count > 0 && !root.highlight-playing;clicked => {root.auto-action(11);}}
                        Button {text: "ライブに戻る / 自動再生を中止";enabled: root.active;clicked => {root.auto-action(9);}}
                        Button {text: "次の試合へ（件数リセット）";enabled: !root.auto-repeat;clicked => {root.auto-action(7);}}
                    }
                }
            }
            if root.show-timer && !root.expanded: HorizontalBox {
                height: 36px;
                Text {text: "RTA " + root.race-time;color: #e2e8f0;font-family: "Consolas";vertical-alignment: center;horizontal-stretch: 1;}
                Button {text: "開始 / 再開";clicked => {root.timer-action(0);}}
                Button {text: "一時停止";clicked => {root.timer-action(1);}}
                Button {text: "リセット";clicked => {root.timer-action(2);}}
            }
            }
            if !root.expanded && !root.settings-open: Rectangle {
                width: 258px;background: #182233;border-radius: 8px;
                VerticalBox {padding: 12px;spacing: 8px;
                    Button {text: root.buffering ? "直近のリプレイを保存" : "リプレイの設定…";enabled: !root.busy;clicked => {if root.buffering {root.save-replay();} else {root.show-settings(3);}}}
                    Rectangle {height: 1px;background: #334155;}
                    HorizontalBox {
                        Text {text: "レイヤー";color: #f8fafc;font-weight: 700;vertical-alignment: center;horizontal-stretch: 1;}
                        Button {text: "追加・設定";clicked => {root.show-settings(1);}}
                    }
                    Text {text: "上が手前 / 下が奥";color: #94a3b8;font-size: 12px;}
                    if root.layers.length == 0: Text {text: "画像や時計を重ねられます。\n「追加・設定」から始めましょう。";color: #cbd5e1;wrap: word-wrap;}
                    ScrollView {min-height: 70px;vertical-stretch: 1;
                        VerticalBox {padding: 0px;spacing: 3px;
                            for label[row] in root.layers: Rectangle {
                                height: 34px;border-radius: 4px;background: root.layer-index == root.layers.length - 1 - row ? #164e63 : #243247;
                                HorizontalBox {padding: 6px;Text {text: label;color: #f1f5f9;overflow: elide;}}
                                TouchArea {clicked => {root.studio-page=1;root.layer-select(root.layers.length - 1 - row);}}
                            }
                        }
                    }
                    CheckBox {text: "選択レイヤーの位置をロック";checked <=> root.layer-locked;enabled: root.layer-index >= 0 && !root.recording && !root.streaming && !root.busy;toggled => {root.layer-lock(self.checked);}}
                    Button {text: "映像・音声の設定…";clicked => {root.show-settings(0);}}

                }
            }
            }
            if root.settings-open && !root.expanded: HorizontalBox {
                height: 36px;
                Text {text: ["映像・音声・保存先", "レイヤーの追加・設定", "YouTube・配信先", "リプレイ・画像認識", "SmashReplayLab連携", "プラグイン", "スマホ・別PC", "Discord・仮想カメラ", "使い方・このアプリについて"][root.studio-page];color: #f8fafc;font-size: 17px;font-weight: 700;vertical-alignment: center;horizontal-stretch: 1;}
                if (root.studio-page >= 1 && root.studio-page <= 3) || root.studio-page == 5 || root.studio-page == 7: Button {primary: true;text: "入力の変更を適用";enabled: !root.recording && !root.streaming && !root.busy;clicked => {root.start-preview();}}
                if root.studio-page == 0: Button {primary: true;text: "保存してプレビューに反映";enabled: !root.recording && !root.streaming && !root.busy;clicked => {if root.save-preferences() {root.start-preview();}}}
                Button {text: "設定を閉じる";clicked => {root.show-workspace();}}
            }
            if root.settings-open && !root.expanded: ScrollView {
                preferred-height: root.studio-page == 0 ? 440px : 255px;max-height: root.studio-page == 0 ? 520px : 290px;
                min-height: 160px;vertical-stretch: 0;
                VerticalBox {padding: 0px;spacing: 8px;
            if root.studio-page == 8: VerticalBox {
                Text {text: "はじめての録画・配信";color: white;font-size: 20px;font-weight: 700;}
                Text {text: "1. 上の「映像入力」で画面またはキャプチャー機器を選びます。\n2. 上の「プレビュー開始」で映像を確認します。音は短い録画で確認します。\n3. 「録画開始」で保存。「設定」→「YouTube・配信先」でYouTubeの配信先とキーを入力します。\n4. 終了時は上の停止ボタン。保存した動画は「ファイル」メニューから開けます。";color: #e2e8f0;wrap: word-wrap;}
                Text {text: "画像・HTML・時計の追加は右のレイヤー一覧から。選択してロックを解除すると、プレビュー上で位置を動かせます（録画・配信・自動認識中は移動できません）。";color: #cbd5e1;wrap: word-wrap;}
                HorizontalBox {
                    Button {text: "映像・音声を設定";clicked => {root.show-settings(0);}}
                    Button {text: "YouTubeを設定";clicked => {root.show-settings(2);}}
                    Button {text: "スマホを接続";clicked => {root.show-settings(6);}}
                }
                Text {text: "Stream Smash 0.20.4 / オープンソース / Windows版\n配信開始はYouTubeへの送信です。公開状態はYouTube Studioでも確認してください。";color: #94a3b8;wrap: word-wrap;}
            }
            if root.studio-page == 6: VerticalBox {
                Text {text: "スマホ・別PC リモコン";color: #f8fafc;font-size: 20px;font-weight: 700;}
                Text {text: "同じWi-Fi/LANから状態確認・リプレイ操作・RTAタイマーを使えます。接続は起動ごとにOFFです。";color: #cbd5e1;wrap: word-wrap;}
                HorizontalBox {
                    LineEdit {text <=> root.phone-ip;placeholder-text: "PCのIPv4アドレス";enabled: !root.phone-enabled;}
                    Button {text: "PCのIPを調べる";enabled: !root.phone-enabled;clicked => {root.phone-action(7);}}
                    SpinBox {value <=> root.phone-port;minimum: 1024;maximum: 65535;enabled: !root.phone-enabled;}
                    Button {text: "スマホ接続 ON";enabled: !root.phone-enabled;clicked => {root.phone-action(0);}}
                    Button {text: "OFF / 全端末を切断";enabled: root.phone-enabled;clicked => {root.phone-action(1);}}
                }
                Text {text: root.phone-status;color: #67e8f9;wrap: word-wrap;}
                if root.phone-enabled: VerticalBox {
                    HorizontalBox {
                        Image {source: root.phone-qr;width: 180px;height: 180px;image-fit: contain;}
                        VerticalBox {
                            Text {text: "① QRをスマホで開く（HTTPS）";color: white;}
                            Text {text: root.phone-url;color: #67e8f9;wrap: word-wrap;}
                            Text {text: "② PCの接続コードをスマホへ入力";color: white;}
                            Text {text: root.phone-code;color: #fde68a;wrap: word-wrap;font-size: 21px;}
                            Button {text: "接続コードを再発行（2分間）";clicked => {root.phone-action(2);}}
                        }
                    }
                    Text {text: "初回はこのPC専用の証明書の確認が必要です。URL・証明書の指紋が一致する接続だけを許可してください。OSの信頼設定は自動変更しません。";color: #cbd5e1;wrap: word-wrap;}
                    Text {text: "SHA-256: " + root.phone-fingerprint;color: #94a3b8;wrap: word-wrap;font-size: 12px;}
                    Text {text: root.phone-pending == "" ? "③ スマホからの接続申請を待っています" : "③ 接続申請：" + root.phone-pending;color: white;wrap: word-wrap;}
                    HorizontalBox {
                        CheckBox {text: "許可する端末に操作を認める（OFF：閲覧のみ）";checked <=> root.phone-allow-control;}
                        Button {text: "接続を許可";enabled: root.phone-pending != "";clicked => {root.phone-action(3);}}
                        Button {text: "拒否";enabled: root.phone-pending != "";clicked => {root.phone-action(4);}}
                    }
                    HorizontalBox {
                        ComboBox {model: root.phone-devices;current-index <=> root.phone-device-index;horizontal-stretch: 1;}
                        Button {text: "選択端末を切断";enabled: root.phone-devices.length > 0;clicked => {root.phone-action(6);}}
                        Button {text: "全端末を失効";clicked => {root.phone-action(5);}}
                    }
                }
                Text {text: "接続できない場合はPCと同じWi-Fiか確認してください。ファイアウォールの許可はプライベートネットワークだけに限定し、設定は自動変更しません。スマホカメラ・大会スコアは後続の機能です。";color: #94a3b8;wrap: word-wrap;}
            }
            if root.studio-page == 5: VerticalBox {
                Text {text: "プラグイン（画像・HTMLオーバーレイ）";color: #f8fafc;font-size: 20px;font-weight: 700;}
                Text {text: "ZIPは展開し、plugin.jsonを含むフォルダーを選択してください。導入後、シーンに追加して入力を適用します。";color: #cbd5e1;wrap: word-wrap;}
                HorizontalBox {
                    Button {text: "フォルダーを選んで確認";enabled: !root.plugin-busy;clicked => {root.plugin-action(0);}}
                    Button {text: "確認したプラグインを導入";enabled: root.plugin-candidate && !root.plugin-busy;clicked => {root.plugin-action(1);}}
                    Button {text: "一覧を更新";enabled: !root.plugin-busy;clicked => {root.plugin-action(2);}}
                    Button {text: "保存フォルダーを開く";clicked => {root.plugin-action(5);}}
                }
                HorizontalBox {
                    ComboBox {model: root.plugin-names;enabled: !root.plugin-busy;current-index <=> root.plugin-index;horizontal-stretch: 1;selected(value) => {root.plugin-action(6);}}
                    Button {text: "シーンに追加 / 有効化";enabled: !root.plugin-busy && root.plugin-names.length > 0 && !root.recording && !root.streaming && !root.busy && !root.auto-monitoring;clicked => {root.plugin-action(3);}}
                    Button {text: "無効化";enabled: !root.plugin-busy && root.plugin-names.length > 0 && !root.recording && !root.streaming && !root.busy && !root.auto-monitoring;clicked => {root.plugin-action(4);}}
                }
                Text {text: root.plugin-details;color: #e2e8f0;wrap: word-wrap;}
                Text {text: "導入だけでは起動しません。HTMLは外部通信する場合があります。信頼する配布元のパッケージを選んでください。";color: #fde68a;wrap: word-wrap;}
                Text {text: "追加後はレイヤー一覧で位置・サイズ・順番・ロックを調整できます。最大8レイヤー。OBS用プラグインや音声/映像フィルターはまだ読み込めません。";color: #94a3b8;wrap: word-wrap;}
            }
            if root.studio-page == 0: VerticalBox {
                HorizontalBox {
                    Button {text: "かんたん設定";primary: root.basic-tab == 0;clicked => {root.basic-tab=0;}}
                    Button {text: "映像を細かく設定";primary: root.basic-tab == 1;clicked => {root.basic-tab=1;}}
                    Button {text: "音声を細かく設定";primary: root.basic-tab == 2;clicked => {root.basic-tab=2;}}
                    Rectangle {horizontal-stretch: 1;}
                }
                if root.basic-tab == 0: VerticalBox {
                    Text {text: "① 上の映像入力を選ぶ → ② 画質と音を選ぶ → ③ プレビューで確認";color: #f8fafc;wrap: word-wrap;}
                    HorizontalBox {
                        Text {text: "画質";color: white;vertical-alignment: center;}
                        Button {text: "軽さ優先（720p / 30）";enabled: !root.recording && !root.streaming && !root.busy;clicked => {root.settings-preset(0);}}
                        Button {text: "くっきり（1080p / 30）";enabled: !root.recording && !root.streaming && !root.busy;clicked => {root.settings-preset(1);}}
                        Button {text: "なめらか（1080p / 60）";enabled: !root.recording && !root.streaming && !root.busy;clicked => {root.settings-preset(2);}}
                    }
                    Text {text: "現在：" + (root.quality-index == 0 ? "1280 × 720 / 30 fps" : root.quality-index == 1 ? "1920 × 1080 / 30 fps" : root.quality-index == 2 ? "1920 × 1080 / 60 fps" : "カスタム " + root.custom-width + " × " + root.custom-height + " / " + root.custom-fps + " fps") + "。なめらかな映像ほどPCと回線の余裕が必要です。";color: #cbd5e1;wrap: word-wrap;}
                }
                if root.basic-tab == 0 || root.basic-tab == 1: VerticalBox {
                    HorizontalBox {
                        Text {text: "映像の圧縮";color: white;vertical-alignment: center;}
                        ComboBox {model: ["CPU（互換性重視）", "AMD GPU", "NVIDIA GPU", "Intel GPU"];current-index <=> root.encoder-index;enabled: !root.recording && !root.streaming && !root.busy && !root.encoder-probing;horizontal-stretch: 1;}
                        Button {text: root.encoder-probing ? "GPUを確認中…" : "使えるGPUを確認";enabled: !root.recording && !root.streaming && !root.busy && !root.encoder-probing;clicked => {root.probe-encoder();}}
                    }
                    Text {text: root.encoder-note;color: #94a3b8;wrap: word-wrap;}
                }
                if root.basic-tab == 0 || root.basic-tab == 2: VerticalBox {
                    HorizontalBox {
                        Text {text: "入れる音";color: white;vertical-alignment: center;}
                        ComboBox {model: root.audio-sources;current-index <=> root.audio-index;enabled: !root.recording && !root.streaming && !root.busy;horizontal-stretch: 1;}
                        Text {text: "音量 %";color: white;vertical-alignment: center;}
                        SpinBox {width: 110px;value <=> root.audio-gain;minimum: 0;maximum: 200;enabled: root.audio-index != 1 && !root.recording && !root.streaming && !root.busy;}
                        CheckBox {text: "ミュート";checked <=> root.primary-muted;enabled: root.audio-index != 1 && !root.recording && !root.streaming && !root.busy;}
                    }
                    HorizontalBox {
                        CheckBox {text: "マイクを追加";checked <=> root.microphone-enabled;enabled: !root.recording && !root.streaming && !root.busy;toggled => {if self.checked {root.audio-limiter=true;}}}
                        ComboBox {model: root.microphones;current-index <=> root.microphone-index;enabled: root.microphone-enabled && !root.recording && !root.streaming && !root.busy;horizontal-stretch: 1;}
                        Text {text: "声の音量 %";color: white;vertical-alignment: center;}
                        SpinBox {width: 110px;value <=> root.microphone-gain;minimum: 0;maximum: 200;enabled: root.microphone-enabled && !root.recording && !root.streaming && !root.busy;}
                        CheckBox {text: "ミュート";checked <=> root.microphone-muted;enabled: root.microphone-enabled && !root.recording && !root.streaming && !root.busy;}
                    }
                    Text {text: root.microphone-enabled ? "選んだ音とマイクを一緒に録画・配信します。声もリプレイへ保存されます。マイク追加時は音割れ防止もONになります。" : "PCのゲーム音やHDMIの音に、自分の声を追加できます。マイクは起動ごとにOFF。録画・配信・リプレイの開始時に取り込みます。";color: #cbd5e1;wrap: word-wrap;}
                    if root.microphone-enabled && (root.microphone-muted || root.microphone-gain == 0): Text {text: "現在、マイクは無音にする設定です。";color: #fde68a;}
                    if root.basic-tab == 2 && root.microphone-enabled: HorizontalBox {
                        Text {text: "ゲーム音に対する声の補正 ms";color: white;vertical-alignment: center;}
                        SpinBox {value <=> root.microphone-offset-ms;minimum: -3000;maximum: 3000;enabled: !root.recording && !root.streaming && !root.busy;}
                    }
                    if root.basic-tab == 2: Text {text: "音量100%が元の大きさ。声がゲーム音より遅い場合はマイク補正をマイナスへ。調整は開始前に行い、短い録画で確認してください。";color: #94a3b8;wrap: word-wrap;}

                }
                if root.basic-tab == 0: VerticalBox {
                    HorizontalBox {
                        Text {text: "動画の保存先";color: white;vertical-alignment: center;}
                        LineEdit {text <=> root.folder;enabled: !root.recording && !root.streaming && !root.busy;horizontal-stretch: 1;}
                        Button {text: "選択…";enabled: !root.recording && !root.streaming && !root.busy && !root.record-folder-picking;clicked => {root.choose-record-folder();}}
                        Button {text: "開く";clicked => {root.open-folder();}}
                    }
                    Button {text: "録画して映像と音を確認";enabled: !root.recording && !root.streaming && !root.busy;clicked => {root.start-recording();root.show-workspace();}}
                    Text {text: "数秒録画して停止 → 「ファイル」から保存先を開いて確認。設定は「設定を保存」で次回へ引き継ぎます。";color: #94a3b8;wrap: word-wrap;}
                }
                if root.basic-tab == 1: VerticalBox {
                    HorizontalBox {
                        Text {text: "出力する映像";color: white;vertical-alignment: center;}
                        ComboBox {model: ["720p / 30 fps", "1080p / 30 fps", "1080p / 60 fps", "カスタム"];current-index <=> root.quality-index;enabled: !root.recording && !root.streaming && !root.busy;horizontal-stretch: 1;}
                    }
                    if root.quality-index == 3: HorizontalBox {
                        Text {text: "幅";color: white;} SpinBox {value <=> root.custom-width;minimum: 320;maximum: 3840;enabled: !root.recording && !root.streaming && !root.busy;}
                        Text {text: "高さ";color: white;} SpinBox {value <=> root.custom-height;minimum: 180;maximum: 2160;enabled: !root.recording && !root.streaming && !root.busy;}
                        Text {text: "fps";color: white;} SpinBox {value <=> root.custom-fps;minimum: 1;maximum: 60;enabled: !root.recording && !root.streaming && !root.busy;}
                    }
                    Text {text: "録画・配信の出力サイズです。幅と高さは偶数にしてください。キャプチャー機器自体の入力モードを変更する機能ではありません。";color: #94a3b8;wrap: word-wrap;}
                    Text {text: "搭載GPUに合う方式を選ぶとCPU負荷を減らせる場合があります。自動判定・自動切替は行いません。開始できない場合はCPUへ戻してください。";color: #94a3b8;wrap: word-wrap;}
                    HorizontalBox {
                        Text {text: "CPU録画品質 CRF";color: white;} SpinBox {value <=> root.record-crf;minimum: 14;maximum: 35;enabled: root.encoder-index == 0 && !root.recording && !root.streaming && !root.busy;}
                        Text {text: "GPU録画 kbps";color: white;} SpinBox {value <=> root.gpu-record-kbps;minimum: 1000;maximum: 100000;enabled: root.encoder-index != 0 && !root.recording && !root.streaming && !root.busy;}
                    }
                    Text {text: "CPUは数値を小さく、GPUは数値を大きくすると高画質・大容量になります。配信の映像量は「YouTube・配信先」で別に設定します。リプレイの映像量は固定です。";color: #94a3b8;wrap: word-wrap;}
                    HorizontalBox {
                        Text {text: "確認用プレビュー";color: white;vertical-alignment: center;}
                        ComboBox {model: ["高画質 1080p / 30", "標準 720p / 30", "省電力 360p / 15"];current-index <=> root.preview-index;enabled: !root.recording && !root.streaming && !root.busy;horizontal-stretch: 1;}
                    }
                    Text {text: "プレビューの設定だけでは、保存する動画の画質は変わりません。";color: #94a3b8;wrap: word-wrap;}
                }
                if root.basic-tab == 2: VerticalBox {
                    CheckBox {text: "音割れを抑える（リミッター）";checked <=> root.audio-limiter;enabled: !root.recording && !root.streaming && !root.busy;}
                    Text {text: "音量100%が元の大きさ、0%で無音です。入力機器ですでに割れた音は修復できません。プレビューは音を再生しないため、短い録画で確認してください。";color: #94a3b8;wrap: word-wrap;}
                    HorizontalBox {
                        Text {text: "録画ファイル";color: white;vertical-alignment: center;}
                        ComboBox {model: ["MKV / Opus（標準）", "MP4 / AAC（編集ソフト向け）"];current-index <=> root.audio-format-index;enabled: !root.recording && !root.streaming && !root.busy;horizontal-stretch: 1;}
                        Text {text: "音声の品質";color: white;}
                        ComboBox {model: ["自動（標準）","64 kbps","96 kbps","128 kbps","160 kbps","192 kbps","256 kbps","320 kbps"];current-index: root.audio-kbps==0 ? 0 : root.audio-kbps==64 ? 1 : root.audio-kbps==96 ? 2 : root.audio-kbps==128 ? 3 : root.audio-kbps==160 ? 4 : root.audio-kbps==192 ? 5 : root.audio-kbps==256 ? 6 : 7;enabled: !root.recording && !root.streaming && !root.busy;selected(value) => {root.audio-kbps=[0,64,96,128,160,192,256,320][self.current-index];}}
                    }
                    Text {text: "「自動」は標準品質（Opus 160 / AAC 192 kbps）。大きい値ほど音声のデータ量が増えます。各音量を調整して混ぜた音を録画・配信・リプレイへ渡します。48kHzは固定。録画のチャンネル構成は入力経路に従い、配信はステレオです。";color: #94a3b8;wrap: word-wrap;}
                    HorizontalBox {
                        Text {text: "音ズレの補正 ms";color: white;vertical-alignment: center;}
                        SpinBox {value <=> root.audio-offset-ms;minimum: -3000;maximum: 3000;enabled: !root.recording && !root.streaming && !root.busy;}
                    }
                    Text {text: "音が遅い → マイナス（例：-100）。音が早い → プラス（例：100）。最初は0にして、録画で確かめながら少しずつ調整します。";color: #cbd5e1;wrap: word-wrap;}
                }
                HorizontalBox {
                    Button {text: "設定を保存";enabled: !root.recording && !root.streaming && !root.busy;clicked => {root.save-preferences();}}
                    Button {text: "映像・音声を標準に戻す";enabled: !root.recording && !root.streaming && !root.busy;clicked => {root.settings-defaults();}}
                }
                Text {text: root.settings-note;color: #67e8f9;wrap: word-wrap;}
            }
            if !root.expanded && root.studio-page == 1: VerticalBox {
                HorizontalBox {
                    Button {text: "元に戻す";enabled: root.editor-can-undo && !root.recording && !root.streaming && !root.busy;clicked => {root.editor-history(false);}}
                    Button {text: "やり直す";enabled: root.editor-can-redo && !root.recording && !root.streaming && !root.busy;clicked => {root.editor-history(true);}}
                    Text {text: "レイヤー編集・シーン読込の履歴 / 最大32操作";color: #94a3b8;}
                }
                spacing: 6px;
                HorizontalBox {
                    Text {text: "レイヤー一覧：上が前面 / 下が背面";color: #f8fafc;horizontal-stretch: 1;}
                    Button { text: "↑ 前面へ"; enabled: !root.recording && !root.streaming && !root.busy; clicked => { root.layer-move(1); } }
                    Button { text: "↓ 背面へ"; enabled: !root.recording && !root.streaming && !root.busy; clicked => { root.layer-move(-1); } }
                    Button {text: "複製";enabled: root.layer-index >= 0 && !root.recording && !root.streaming && !root.busy;clicked => {root.layer-duplicate();}}
                    Button { text: "削除"; enabled: !root.recording && !root.streaming && !root.busy; clicked => { root.layer-remove(); } }
                }
                ScrollView {
                    height: 136px;
                    VerticalBox {padding: 0px;spacing: 2px;
                        for label[row] in root.layers: Rectangle {
                            height: 32px;background: root.layer-index == root.layers.length - 1 - row ? #164e63 : #1e293b;
                            HorizontalBox {padding: 6px;Text {text: label;color: #f1f5f9;}}
                            TouchArea {clicked => {root.layer-select(root.layers.length - 1 - row);}}
                        }
                    }
                }
                HorizontalBox {
                    CheckBox {text: "位置をロック";checked <=> root.layer-locked;enabled: root.layer-index >= 0 && !root.recording && !root.streaming && !root.busy;toggled => {root.layer-lock(self.checked);}}
                    Text {text: "一覧で選択 → ロック解除 → プレビューの枠内をドラッグ";color: #67e8f9;wrap: word-wrap;}
                }
                Text {text: root.recording || root.streaming || root.auto-monitoring ? "移動するには録画・配信・自動認識を停止してください" : "離すと位置を確定してプレビューへ反映します。Undoで戻せます。";color: #94a3b8;wrap: word-wrap;}
                HorizontalBox {
                    ComboBox { model: ["画像 PNG / WebP", "HTML / URL", "RTAタイマー", "音声波形"]; current-index <=> root.layer-kind; }
                    LineEdit { text <=> root.layer-name; placeholder-text: "名前"; }
                    Button { text: "追加（画像は選択）"; enabled: !root.recording && !root.streaming && !root.busy; clicked => { root.layer-add(); } }
                    Button { text: "シーン読込"; enabled: !root.recording && !root.streaming && !root.busy; clicked => { root.scene-load(); } }
                    Button { text: "シーン保存"; clicked => { root.scene-save(); } }
                }
                LineEdit { text <=> root.layer-location; placeholder-text: "HTMLのURL / ローカルHTML・画像のパス（タイマー・波形では不要）"; }
                HorizontalBox {
                    Text {text: "X";color: white;} SpinBox {value <=> root.layer-x; minimum: -3840; maximum: 3840;}
                    Text {text: "Y";color: white;} SpinBox {value <=> root.layer-y; minimum: -2160; maximum: 2160;}
                    Text {text: "幅";color: white;} SpinBox {value <=> root.layer-width; minimum: 16; maximum: 3840;}
                    Text {text: "高さ";color: white;} SpinBox {value <=> root.layer-height; minimum: 16; maximum: 2160;}
                }
                HorizontalBox {
                    CheckBox {text: "表示";checked <=> root.layer-enabled;}
                    Text {text: "不透明度 %";color: white;} SpinBox {value <=> root.layer-opacity; minimum: 0; maximum: 100;}
                    Button {text: "選択レイヤーを更新";enabled: !root.recording && !root.streaming && !root.busy;clicked => {root.layer-apply();}}
                    Text {text: "配置基準 1920×1080 / 一覧の上が最前面 / 変更後に入力を適用";color: #94a3b8;wrap: word-wrap;}
                }
            }
            if !root.expanded && root.studio-page == 2: VerticalBox {
                LineEdit { text <=> root.stream-server; placeholder-text: "YouTube StudioのRTMPSサーバーURL"; enabled: !root.streaming; }
                LineEdit { text <=> root.stream-key; input-type: password; placeholder-text: "配信キー（保存しません）"; enabled: !root.streaming; }
                HorizontalBox {
                    Text {text: "映像 kbps"; color: white; } SpinBox {value <=> root.bitrate; minimum: 1000; maximum: 20000; enabled: !root.streaming;}
                    CheckBox {text: "同時に録画"; checked <=> root.stream-record; enabled: !root.streaming;}
                }
                Text {text: "配信先とキーを入力したら、上部の「配信開始」を押してください。YouTube Studioでも受信と公開状態を確認します。配信キーは毎回入力します。";color: #94a3b8;wrap: word-wrap;}
            }
            if !root.expanded && root.studio-page == 7: HorizontalBox {
                CheckBox {text: "Discord用仮想カメラ";checked <=> root.virtual-camera;enabled: !root.active && !root.busy;}
                Button {text: "初回登録（管理者）";clicked => {root.install-camera();}}
                Button {text: "カメラ診断";clicked => {root.diagnose-camera();}}
                Button {text: "共有用の映像ウィンドウ";clicked => {root.show-output();}}
            }
            if !root.expanded && root.studio-page == 7: VerticalBox {
                Text {text: root.camera-status;color: #67e8f9;wrap: word-wrap;}
                CheckBox {text: "仮想カメラの出力を左右反転";checked <=> root.virtual-camera-mirror;enabled: root.virtual-camera && !root.active && !root.busy;}
                Text {text: "相手に届く映像も反転します。通常はOFF。変更する場合は停止して設定後、プレビューを開始してください。";color: #94a3b8;wrap: word-wrap;}
            }
            if !root.expanded && root.studio-page == 3: HorizontalBox {
                CheckBox {text: "リプレイを有効化";checked <=> root.replay-enabled;enabled: !root.active && !root.busy;}
                SpinBox {value <=> root.replay-seconds;minimum: 10;maximum: 600;enabled: !root.active && !root.busy;}
                Text {text: "秒 / 開始は入力を適用";color: #94a3b8;}
                Button {text: "直近のリプレイを保存";enabled: root.buffering && !root.busy;clicked => {root.save-replay();}}
            }
            if !root.expanded && root.studio-page == 4: VerticalBox {
                CheckBox {text: "SmashReplayLabとの連携を有効化";checked <=> root.replaylab-enabled;toggled => {root.toggle-replaylab(self.checked);}}
                Button {text: "SmashReplayLabを開く";enabled: root.replaylab-enabled;clicked => {root.launch-replaylab();}}
                Text {text: root.replaylab-status;color: #e2e8f0;wrap: word-wrap;}
                Text {text: "リプレイを有効化して秒数を指定 → プレビュー開始 → ReplayLabで接続ファイルを選択。GAME SETの自動保存はReplayLabで開始します。";color: #94a3b8;wrap: word-wrap;}
                Text {text: "取得映像は合成後の640×360。文字や画像でGAME SETを隠さないでください。試合全体は必要に応じて600秒まで指定（保存時間分の空き容量が必要）。";color: #94a3b8;wrap: word-wrap;}
            }
            if !root.expanded && root.studio-page == 3: VerticalBox {
                HorizontalBox {
                    Text {text: "リプレイ動画の保存先";color: #e2e8f0;vertical-alignment: center;}
                    LineEdit {text <=> root.replay-folder;horizontal-stretch: 1;enabled: !root.busy;}
                    Button {text: "選択…";enabled: !root.replay-folder-picking && !root.busy;clicked => {root.choose-replay-folder();}}
                    Button {text: "適用";enabled: !root.busy;clicked => {root.apply-replay-folder();}}
                    Button {text: "開く";clicked => {root.open-replay-folder();}}
                }
                HorizontalBox {
                    ComboBox {model: ["認識・保存", "学習・範囲調整"];current-index <=> root.auto-page;}
                    Text {text: "認識画像は640×360 / 範囲はその画像上の座標";color: #94a3b8;}
                }
                if root.auto-page == 0: VerticalBox {
                    Text {text: root.auto-monitoring ? "動作中：GAME SETでこの試合を区切ります" : root.buffering ? "準備OK：認識STARTで開始します" : "準備：リプレイを有効化 → プレビュー開始 → 認識START";color: root.buffering ? #8bf0ac : #fde68a;wrap: word-wrap;}
                    HorizontalBox {
                        Text {text: "かんたん設定";color: #f8fafc;}
                        Button {text: "標準（5秒 / 余韻1秒）";enabled: !root.auto-busy;clicked => {root.auto-action(14);}}
                        Button {text: "余韻重視（8秒 / 余韻3秒）";enabled: !root.auto-busy;clicked => {root.auto-action(15);}}
                        Button {text: "手動で再生";enabled: !root.auto-busy;clicked => {root.auto-action(16);}}
                    }
                    Text {text: root.highlight-autoplay ? "自動再生：GAME SET消失から約 " + root.highlight-delay + " 秒 / 表示後の余韻 " + root.highlight-post-roll + " 秒" : "手動再生：保存後に「今すぐ再生」を押します / 表示後の余韻 " + root.highlight-post-roll + " 秒";color: #cbd5e1;wrap: word-wrap;}
                    HorizontalBox {
                        ComboBox {model: ["GAME SET", "撃墜（－表示）", "両方"];current-index <=> root.auto-detection;enabled: !root.auto-busy;}
                        Text {text: "撃墜クリップ 秒";color: white;}
                        SpinBox {value <=> root.highlight-seconds;minimum: 6;maximum: 60;enabled: !root.auto-busy;}
                    }
                    HorizontalBox {
                        CheckBox {text: "毎試合続ける（OFF：1試合で停止）";checked <=> root.auto-repeat;enabled: !root.auto-busy;}
                    }
                    HorizontalBox {
                        Button {text: root.auto-repeat ? "連続認識 START" : "この試合の認識 START";enabled: root.buffering && !root.busy && !root.auto-busy;clicked => {root.auto-action(5);}}
                        Button {text: "認識だけ停止";enabled: root.auto-busy;clicked => {root.auto-action(6);}}
                        Button {text: "リプレイ全停止";enabled: root.buffering && !root.recording && !root.streaming && !root.busy;clicked => {root.auto-action(17);}}
                    }
                    HorizontalBox {
                        CheckBox {text: "細かく設定する";checked <=> root.auto-details;}
                        Button {text: "設定を保存";enabled: !root.auto-busy;clicked => {root.auto-action(12);}}
                        Text {text: "認識START時にも保存されます";color: #94a3b8;}
                    }
                    if root.auto-details: VerticalBox {
                        CheckBox {text: "テスト専用（保存・再生しない）";checked <=> root.auto-dry;enabled: !root.auto-busy;}
                    HorizontalBox {
                        CheckBox {text: "試合終了後に自動再生";checked <=> root.highlight-autoplay;enabled: !root.auto-busy;}
                        Text {text: "GAME SET消失後";color: white;}
                        SpinBox {value <=> root.highlight-delay;minimum: 0;maximum: 120;enabled: !root.auto-busy;}
                        Text {text: "秒";color: white;}
                    }
                    HorizontalBox {
                        Text {text: "表示が消えた後も保存";color: #e5e7eb;}
                        SpinBox {value <=> root.highlight-post-roll;minimum: 0;maximum: 10;enabled: !root.auto-busy;}
                        Text {text: "秒";color: white;}
                    }
                    }
                    if root.auto-dry: Text {text: "テスト専用ON：クリップは保存・再生されません";color: #fde68a;}
                    CheckBox {text: "TV風 REPLAY表示を配信映像に重ねる";checked <=> root.replay-tv-overlay;toggled => {root.auto-action(13);}}
                    Text {text: "配信映像そのものがリプレイへ切り替わります。録画・共有用映像・仮想カメラにも反映。";color: #cbd5e1;wrap: word-wrap;}
                    if root.auto-details: Text {text: root.auto-score;color: #e2e8f0;}
                    Text {text: "STARTでこの試合の認識開始 → GAME SETで区切る → 指定秒後に再生 → ライブ復帰。連続認識ONなら次の試合を自動で待ちます。GAME SETと－の両方の学習が必要です。";color: #94a3b8;wrap: word-wrap;}
                }
                if root.auto-page == 1: VerticalBox {
                    ComboBox {model: ["GAME SET用の学習", "撃墜の－表示用の学習"];current-index <=> root.auto-training;enabled: !root.auto-busy;selected(value) => {root.auto-action(0);}}
                    HorizontalBox {
                        Text {text: "X";color: white;} SpinBox {value <=> root.auto-x;minimum: 0;maximum: 620;enabled: !root.auto-busy;}
                        Text {text: "Y";color: white;} SpinBox {value <=> root.auto-y;minimum: 0;maximum: 350;enabled: !root.auto-busy;}
                        Text {text: "幅";color: white;} SpinBox {value <=> root.auto-width;minimum: 20;maximum: 640;enabled: !root.auto-busy;}
                        Text {text: "高さ";color: white;} SpinBox {value <=> root.auto-height;minimum: 10;maximum: 360;enabled: !root.auto-busy;}
                    }
                    HorizontalBox {
                        Button {text: root.auto-training == 0 ? "今の映像 → GAME SET ＋" : "今の映像 → －表示 ＋";enabled: root.active && !root.busy && !root.auto-busy;clicked => {root.auto-action(1);}}
                        Button {text: "今の映像 → 通常画面 ＋";enabled: root.active && !root.busy && !root.auto-busy;clicked => {root.auto-action(2);}}
                        Button {text: "通常画面を5秒収集";enabled: root.active && !root.busy && !root.auto-busy;clicked => {root.auto-action(3);}}
                    }
                    HorizontalBox {
                        Button {text: "自動調整・範囲を保存";enabled: !root.auto-busy;clicked => {root.auto-action(4);}}
                        Button {text: "学習データを再読込";enabled: !root.auto-busy;clicked => {root.auto-action(0);}}
                        Button {text: "学習画像フォルダー";clicked => {root.auto-action(8);}}
                    }
                    Text {text: root.auto-learning;color: #e2e8f0;wrap: word-wrap;}
                }
            }
                }
            }
            Rectangle {
                background: #1e293b;height: 42px;border-radius: 5px;
                HorizontalBox {padding: 8px;spacing: 12px;
                    ScrollView {horizontal-stretch: 1;Text {text: root.status;color: #e2e8f0;wrap: word-wrap;}}
                    Text {text: root.camera-status;width: 180px;overflow: elide;color: #94a3b8;vertical-alignment: center;font-size: 12px;}
                    Button {text: "使い方";clicked => {root.show-settings(8);}}
                }
            }

        }
    }
}

pub(crate) fn strings(values: impl IntoIterator<Item = String>) -> ModelRc<SharedString> {
    Rc::new(VecModel::from(
        values
            .into_iter()
            .map(SharedString::from)
            .collect::<Vec<_>>(),
    ))
    .into()
}

fn output_dimensions(window: &MainWindow) -> (u32, u32, u32) {
    match window.get_quality_index() {
        1 => (1920, 1080, 30),
        2 => (1920, 1080, 60),
        3 => (
            window.get_custom_width() as u32,
            window.get_custom_height() as u32,
            window.get_custom_fps() as u32,
        ),
        _ => (1280, 720, 30),
    }
}
fn probe_request(w: &MainWindow) -> streamsmash_media::encoder_probe::Request {
    let (width, height, fps) = output_dimensions(w);
    streamsmash_media::encoder_probe::Request { width, height, fps }
}
fn setup_encoder_probe(w: &MainWindow) -> streamsmash_media::encoder_probe::Worker {
    let worker = streamsmash_media::encoder_probe::Worker::new();
    let tx = worker.requests.clone();
    let weak = w.as_weak();
    w.on_probe_encoder(move || {
        let Some(w) = weak.upgrade() else {
            return;
        };
        if w.get_recording() || w.get_streaming() || w.get_busy() || w.get_encoder_probing() {
            return;
        }
        w.set_encoder_probing(true);
        w.set_encoder_note("人工映像でGPU圧縮を確認しています…".into());
        if tx.send(probe_request(&w)).is_err() {
            w.set_encoder_probing(false);
            w.set_encoder_note("GPU確認を開始できませんでした".into());
        }
    });
    worker
}
fn apply_encoder_probe_reply(w: &MainWindow, reply: streamsmash_media::encoder_probe::Reply) {
    w.set_encoder_probing(false);
    if w.get_recording() || w.get_streaming() || w.get_busy() || reply.request != probe_request(w) {
        w.set_encoder_note("確認中に状態が変わったため圧縮設定は変更していません。停止後、現在の画質でもう一度確認してください。".into());
        return;
    }
    match reply.result {
        Ok(selection) => {
            w.set_encoder_index(selection.index());
            let message = if selection.encoder.is_some() {
                let usage = if selection.tested_outputs == 3 {
                    "配信・録画・リプレイ併用の短い確認に成功"
                } else {
                    "配信＋リプレイの短い確認に成功（同時録画は未確認）"
                };
                format!(
                    "{}：{}。「保存してプレビューに反映」で使います。",
                    selection.name(),
                    usage
                )
            } else {
                "利用できるGPU圧縮を確認できなかったためCPUを選びました。".into()
            };
            w.set_encoder_note(message.into());
        }
        Err(error) => w.set_encoder_note(format!("GPU確認に失敗しました：{error}").into()),
    }
}
fn config(
    window: &MainWindow,
    devices: &Devices,
    scene: &streamsmash_media::scene::Scene,
) -> Result<Config, String> {
    let source = match window.get_source_index() {
        0 => Source::Desktop {
            monitor: window.get_monitor() as u32,
        },
        1 => Source::DesktopCompatible,
        index => Source::Device(
            devices
                .video
                .get((index - 2) as usize)
                .ok_or("映像デバイスを選び直してください")?
                .id
                .clone(),
        ),
    };
    let audio = match window.get_audio_index() {
        0 => Audio::System,
        1 => Audio::None,
        index => Audio::Device(
            devices
                .audio
                .get((index - 2) as usize)
                .ok_or("音声デバイスを選び直してください")?
                .id
                .clone(),
        ),
    };
    let (width, height, fps) = output_dimensions(window);
    let encoder = match window.get_encoder_index() {
        1 => Encoder::Amd,
        2 => Encoder::Nvidia,
        3 => Encoder::Intel,
        _ => Encoder::Software,
    };
    let config = Config {
        virtual_camera: window.get_virtual_camera(),
        virtual_camera_mirror: window.get_virtual_camera_mirror(),
        replay_seconds: if window.get_replay_enabled() {
            Some(window.get_replay_seconds() as u32)
        } else {
            None
        },
        scene: scene.clone(),
        recording_format: if window.get_audio_format_index() == 1 {
            RecordingFormat::AacMp4
        } else {
            RecordingFormat::OpusMkv
        },
        tuning: preferences_from_window(window).tuning,
        audio_offset_ms: window.get_audio_offset_ms(),
        preview: match window.get_preview_index() {
            1 => PreviewQuality::Hd,
            2 => PreviewQuality::Economy,
            _ => PreviewQuality::FullHd,
        },
        source,
        audio,
        microphone: if window.get_microphone_enabled() {
            Some(if window.get_microphone_index() == 0 {
                "default".into()
            } else {
                devices
                    .microphones
                    .get((window.get_microphone_index() - 1) as usize)
                    .ok_or("マイクを選び直してください")?
                    .id
                    .clone()
            })
        } else {
            None
        },
        encoder,
        width,
        height,
        fps,
        folder: PathBuf::from(window.get_folder().as_str()),
    };
    config.validate()?;
    Ok(config)
}

fn refresh_microphones(w: &MainWindow, previous: &Devices, found: &Devices) {
    let old = if w.get_microphone_index() > 0 {
        previous
            .microphones
            .get((w.get_microphone_index() - 1) as usize)
            .map(|d| d.id.clone())
    } else {
        None
    };
    w.set_microphones(strings(
        std::iter::once("Windowsの既定マイク".into())
            .chain(found.microphones.iter().map(|d| d.name.clone())),
    ));
    if let Some(id) = old {
        if let Some(index) = found.microphones.iter().position(|d| d.id == id) {
            w.set_microphone_index(index as i32 + 1);
        } else {
            w.set_microphone_index(0);
            w.set_microphone_enabled(false);
            w.set_settings_note(
                "選択していたマイクが見つかりません。マイクを選び直してONにしてください。".into(),
            );
        }
    } else {
        w.set_microphone_index(0);
    }
}

fn preferences_from_window(w: &MainWindow) -> crate::preferences::Preferences {
    crate::preferences::Preferences {
        quality: w.get_quality_index(),
        preview: w.get_preview_index(),
        encoder: w.get_encoder_index(),
        audio_format: w.get_audio_format_index(),
        width: w.get_custom_width() as u32,
        height: w.get_custom_height() as u32,
        fps: w.get_custom_fps() as u32,
        offset_ms: w.get_audio_offset_ms(),
        stream_kbps: w.get_bitrate() as u32,
        tuning: streamsmash_media::tuning::OutputTuning {
            gain_percent: w.get_audio_gain() as u16,
            primary_muted: w.get_primary_muted(),
            mic_gain_percent: w.get_microphone_gain() as u16,
            mic_muted: w.get_microphone_muted(),
            mic_offset_ms: w.get_microphone_offset_ms(),
            limiter: w.get_audio_limiter(),
            audio_kbps: w.get_audio_kbps() as u32,
            record_crf: w.get_record_crf() as u8,
            gpu_record_kbps: w.get_gpu_record_kbps() as u32,
        },
        record_folder: Some(PathBuf::from(w.get_folder().as_str())),
        ..Default::default()
    }
}
fn restore_preferences(w: &MainWindow, p: &crate::preferences::Preferences) {
    w.set_quality_index(p.quality);
    w.set_preview_index(p.preview);
    w.set_encoder_index(p.encoder);
    w.set_audio_format_index(p.audio_format);
    w.set_custom_width(p.width as i32);
    w.set_custom_height(p.height as i32);
    w.set_custom_fps(p.fps as i32);
    w.set_audio_offset_ms(p.offset_ms);
    w.set_bitrate(p.stream_kbps as i32);
    w.set_audio_gain(p.tuning.gain_percent as i32);
    w.set_primary_muted(p.tuning.primary_muted);
    w.set_microphone_gain(p.tuning.mic_gain_percent as i32);
    w.set_microphone_muted(p.tuning.mic_muted);
    w.set_microphone_offset_ms(p.tuning.mic_offset_ms);
    w.set_audio_limiter(p.tuning.limiter);
    w.set_audio_kbps(p.tuning.audio_kbps as i32);
    w.set_record_crf(p.tuning.record_crf as i32);
    w.set_gpu_record_kbps(p.tuning.gpu_record_kbps as i32);
    if let Some(folder) = &p.record_folder {
        w.set_folder(folder.to_string_lossy().as_ref().into());
    }
}
fn setup_preferences_at(w: &MainWindow, path: PathBuf) {
    match crate::preferences::load(&path) {
        Ok(Some(p)) => {
            restore_preferences(w, &p);
            w.set_settings_note("保存した映像・音声設定を読み込みました。音の入力先は機器を確認して選んでください。".into());
        }
        Ok(None) => {}
        Err(e) => w.set_settings_note(
            format!("保存設定を読み込めませんでした。現在の設定を確認してください：{e}").into(),
        ),
    }
    let weak = w.as_weak();
    w.on_settings_preset(move |index| {
        let Some(w)=weak.upgrade() else{return;};
        if w.get_recording() || w.get_streaming() || w.get_busy() || !(0..=2).contains(&index) {return;}
        w.set_quality_index(index);w.set_bitrate([4000,6000,9000][index as usize]);
        w.set_settings_note("画質と配信の映像量を変更しました。音量・音ズレ補正・プレビュー画質は維持します。保存または反映で確定してください。".into());
    });
    let weak = w.as_weak();
    w.on_settings_defaults(move || {
        let Some(w)=weak.upgrade() else{return;};if w.get_recording() || w.get_streaming() || w.get_busy(){return;}
        restore_preferences(&w,&Default::default());
        w.set_status("映像・音声の調整値を標準に戻しました".into());w.set_settings_note("映像・音声の調整値を標準に戻しました。映像入力・音の入力先・保存先は変わりません。保存で確定してください。".into());
    });
    let weak = w.as_weak();
    w.on_save_preferences(move || {
        let Some(w)=weak.upgrade() else{return false;};if w.get_recording() || w.get_streaming() || w.get_busy(){return false;}
        match crate::preferences::save(&path,&preferences_from_window(&w)) {
            Ok(())=>{w.set_status("設定を保存しました。反映は上のプレビューボタンから行えます".into());w.set_settings_note("設定を保存しました。次回起動にも引き継ぎます。映像へ反映する時は上のボタンを押してください。".into());true}
            Err(e)=>{let message=format!("設定を保存できません：{e}");w.set_settings_note(message.clone().into());w.set_status(message.into());false}
        }
    });
}

fn audio_settings_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.join("audio-settings.ini")))
        .unwrap_or_else(|| PathBuf::from("audio-settings.ini"))
}

fn show_layer(w: &MainWindow, scene: &streamsmash_media::scene::Scene, index: i32) {
    use streamsmash_media::scene::LayerSource;
    if let Some(l) = scene.layers.get(index as usize) {
        w.set_layer_index(index);
        w.set_layer_name(l.name.as_str().into());
        w.set_layer_x(l.x);
        w.set_layer_y(l.y);
        w.set_layer_width(l.width as i32);
        w.set_layer_height(l.height as i32);
        w.set_layer_opacity((l.opacity * 100.0).round() as i32);
        w.set_layer_enabled(l.enabled);
        w.set_layer_locked(l.locked);
        let (kind, location) = match &l.source {
            LayerSource::Image { path } => (0, path.to_string_lossy().into_owned()),
            LayerSource::Browser { location } => (1, location.clone()),
            LayerSource::Timer => (2, String::new()),
            LayerSource::Waveform => (3, String::new()),
        };
        w.set_layer_kind(kind);
        w.set_layer_location(location.into());
    }
}
fn refresh_layers(w: &MainWindow, scene: &streamsmash_media::scene::Scene, index: i32) {
    w.set_layers(strings(scene.layers.iter().enumerate().rev().map(
        |(i, l)| {
            format!(
                "{}: {}{} / {}",
                i + 1,
                l.name,
                if l.enabled { "" } else { "（非表示）" },
                if l.locked { "固定" } else { "移動可" }
            )
        },
    )));
    let index = if scene.layers.is_empty() {
        -1
    } else {
        index.max(0).min(scene.layers.len() as i32 - 1)
    };
    w.set_layer_index(index);
    show_layer(w, scene, index);
}
fn edited_layer(w: &MainWindow) -> Result<streamsmash_media::scene::Layer, String> {
    use streamsmash_media::scene::{Layer, LayerSource};
    let source = match w.get_layer_kind() {
        0 => LayerSource::Image {
            path: PathBuf::from(w.get_layer_location().as_str()),
        },
        1 => LayerSource::Browser {
            location: w.get_layer_location().to_string(),
        },
        2 => LayerSource::Timer,
        _ => LayerSource::Waveform,
    };
    let layer = Layer {
        name: w.get_layer_name().to_string(),
        enabled: w.get_layer_enabled(),
        locked: w.get_layer_locked(),
        source,
        x: w.get_layer_x(),
        y: w.get_layer_y(),
        width: w.get_layer_width() as u32,
        height: w.get_layer_height() as u32,
        opacity: w.get_layer_opacity() as f32 / 100.0,
    };
    layer.validate()?;
    Ok(layer)
}
struct PluginManager {
    folder: PathBuf,
    items: Vec<crate::plugins::Installed>,
    candidate: Option<PathBuf>,
    pending: Option<std::sync::mpsc::Receiver<Result<crate::plugins::Installed, String>>>,
}
fn refresh_plugins(w: &MainWindow, manager: &mut PluginManager) {
    let (items, errors) = crate::plugins::list(&manager.folder);
    manager.items = items;
    w.set_plugin_names(strings(
        manager
            .items
            .iter()
            .map(|p| format!("{} / v{}", p.manifest.name, p.manifest.version)),
    ));
    w.set_plugin_index(0);
    w.set_plugin_candidate(false);
    manager.candidate = None;
    w.set_plugin_details(
        if errors.is_empty() {
            manager.items.first().map_or_else(
                || "導入済みプラグインはありません。フォルダーを選んで導入してください。".into(),
                |p| p.manifest.summary(),
            )
        } else {
            format!(
                "読み込めないプラグイン: {}。plugin.jsonを確認してください。",
                errors.join(", ")
            )
        }
        .into(),
    );
}
fn setup_plugins(
    w: &MainWindow,
    scene: Rc<RefCell<streamsmash_media::scene::Scene>>,
    history: Rc<RefCell<crate::scene_history::History>>,
) {
    let folder = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .map(|p| p.join("StreamSmash/plugins"))
        .unwrap_or_else(|| {
            std::env::current_exe()
                .unwrap_or_default()
                .with_file_name("plugins")
        });
    setup_plugins_at(w, scene, history, folder);
}
fn setup_plugins_at(
    w: &MainWindow,
    scene: Rc<RefCell<streamsmash_media::scene::Scene>>,
    history: Rc<RefCell<crate::scene_history::History>>,
    folder: PathBuf,
) {
    let mut manager = PluginManager {
        folder,
        items: vec![],
        candidate: None,
        pending: None,
    };
    refresh_plugins(w, &mut manager);
    let manager = Rc::new(RefCell::new(manager));
    let timer = Rc::new(slint::Timer::default());
    let weak = w.as_weak();
    let state = manager.clone();
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(100),
        move || {
            let Some(w) = weak.upgrade() else {
                return;
            };
            let Ok(mut manager) = state.try_borrow_mut() else {
                return;
            };
            let result = manager.pending.as_ref().and_then(|rx| match rx.try_recv() {
                Ok(result) => Some(result),
                Err(std::sync::mpsc::TryRecvError::Disconnected) => Some(Err(
                    "導入処理が終了しました。もう一度フォルダーを選択してください".into(),
                )),
                _ => None,
            });
            if let Some(result) = result {
                manager.pending = None;
                w.set_plugin_busy(false);
                match result {
                    Ok(installed) => {
                        refresh_plugins(&w, &mut manager);
                        if let Some(index) = manager
                            .items
                            .iter()
                            .position(|item| item.manifest.id == installed.manifest.id)
                        {
                            w.set_plugin_index(index as i32);
                        }
                        w.set_plugin_details(
                            format!(
                                "{}を導入しました。シーンに追加すると使用できます。",
                                installed.manifest.name
                            )
                            .into(),
                        );
                    }
                    Err(error) => {
                        w.set_plugin_details(format!("導入できませんでした: {error}").into())
                    }
                }
            }
        },
    );
    let weak = w.as_weak();
    w.on_plugin_action(move |action| {
        let _keep_timer=&timer;
        let Some(w)=weak.upgrade() else{return;};if w.get_plugin_busy(){return;}let mut manager=manager.borrow_mut();
        let result=(||->Result<(),String>{
            match action {
                0=>{
                    manager.candidate=None;w.set_plugin_candidate(false);
                    #[cfg(windows)]
                    if let Some(folder)=rfd::FileDialog::new().set_title("plugin.jsonを含むフォルダーを選択").pick_folder(){
                        let manifest=crate::plugins::Manifest::read(&folder)?;w.set_plugin_details(manifest.summary().into());manager.candidate=Some(folder);w.set_plugin_candidate(true);
                    }
                }
                1=>{
                    let folder=manager.candidate.take().ok_or("先にプラグインのフォルダーを選んでください")?;w.set_plugin_candidate(false);
                    let store=manager.folder.clone();let (tx,rx)=std::sync::mpsc::channel();manager.pending=Some(rx);
                    w.set_plugin_busy(true);w.set_plugin_details("プラグインを導入しています…".into());
                    std::thread::spawn(move || {let result=crate::plugins::install(&folder,&store);let _=tx.send(result);});
                }
                2=>refresh_plugins(&w,&mut manager),
                3|4=>{
                    if w.get_recording() || w.get_streaming() || w.get_busy() || w.get_auto_monitoring(){return Err("録画・配信・自動認識を停止してから変更してください".into());}
                    let plugin=manager.items.get(w.get_plugin_index() as usize).ok_or("プラグインを選んでください")?;
                    let before=crate::scene_history::Snapshot{scene:scene.borrow().clone(),selected:w.get_layer_index()};let mut updated=before.scene.clone();let mut selected=None;
                    for (i,layer) in updated.layers.iter_mut().enumerate(){if crate::plugins::owns(plugin,layer){layer.enabled=action==3;selected=Some(i as i32);}}
                    if selected.is_none() && action==3 {
                        if updated.layers.len()>=8 {return Err("レイヤーは最大8個です".into());}
                        let manifest=crate::plugins::Manifest::read(&plugin.folder)?;updated.layers.push(manifest.layer(&plugin.folder)?);selected=Some(updated.layers.len() as i32-1);
                    }
                    if selected.is_none() && action==4 {return Err("このプラグインは現在のシーンに追加されていません".into());}
                    updated.validate()?;history.borrow_mut().record(before,&updated);*scene.borrow_mut()=updated;
                    w.set_editor_can_undo(history.borrow().can_undo());w.set_editor_can_redo(history.borrow().can_redo());refresh_layers(&w,&scene.borrow(),selected.unwrap_or(w.get_layer_index()));
                    w.set_plugin_details(if action==3 {"シーンへ反映しました。「入力を適用」で表示します。レイヤー合成で位置・サイズを調整できます。"}else{"レイヤーを非表示にしました。「入力を適用」で映像にも反映します。"}.into());
                }
                5=>{std::fs::create_dir_all(&manager.folder).map_err(|e|e.to_string())?;crate::folders::open(&manager.folder)?;}
                6=>{manager.candidate=None;w.set_plugin_candidate(false);if let Some(plugin)=manager.items.get(w.get_plugin_index() as usize){w.set_plugin_details(plugin.manifest.summary().into());}}
                _=>{}
            }
            Ok(())
        })();
        if let Err(error)=result{w.set_plugin_details(format!("プラグイン: {error}").into());}
    });
}

fn setup_studio(
    w: &MainWindow,
    scene: Rc<RefCell<streamsmash_media::scene::Scene>>,
    devices: Rc<RefCell<Devices>>,
    controller: Rc<Controller>,
) {
    let history = Rc::new(RefCell::new(crate::scene_history::History::default()));
    setup_plugins(w, scene.clone(), history.clone());
    let weak = w.as_weak();
    let state = scene.clone();
    let changes = history.clone();
    w.on_editor_history(move |redo| {
        let Some(w) = weak.upgrade() else {
            return;
        };
        if w.get_recording() || w.get_streaming() || w.get_busy() {
            return;
        }
        let current = crate::scene_history::Snapshot {
            scene: state.borrow().clone(),
            selected: w.get_layer_index(),
        };
        let previous = if redo {
            changes.borrow_mut().redo(current)
        } else {
            changes.borrow_mut().undo(current)
        };
        if let Some(snapshot) = previous {
            *state.borrow_mut() = snapshot.scene;
            refresh_layers(&w, &state.borrow(), snapshot.selected);
            w.set_status(
                if redo {
                    "やり直しました。入力を適用すると映像へ反映します。"
                } else {
                    "元に戻しました。入力を適用すると映像へ反映します。"
                }
                .into(),
            );
        }
        w.set_editor_can_undo(changes.borrow().can_undo());
        w.set_editor_can_redo(changes.borrow().can_redo());
    });
    let weak = w.as_weak();
    let state = scene.clone();
    w.on_layer_select(move |index| {
        if let Some(w) = weak.upgrade() {
            show_layer(&w, &state.borrow(), index);
        }
    });
    for drag in [false, true] {
        let weak = w.as_weak();
        let state = scene.clone();
        let changes = history.clone();
        let edit = move |locked: bool| {
            let Some(w) = weak.upgrade() else {
                return;
            };
            if w.get_recording()
                || w.get_streaming()
                || w.get_busy()
                || (drag && w.get_auto_monitoring())
            {
                return;
            }
            let index = w.get_layer_index();
            let before = crate::scene_history::Snapshot {
                scene: state.borrow().clone(),
                selected: index,
            };
            let mut updated = before.scene.clone();
            let Some(layer) = updated.layers.get_mut(index as usize) else {
                return;
            };
            if drag {
                if layer.locked {
                    show_layer(&w, &before.scene, index);
                    return;
                }
                layer.x = w.get_layer_x().clamp(-3840, 3840);
                layer.y = w.get_layer_y().clamp(-2160, 2160);
            } else {
                layer.locked = locked;
            }
            let changed = updated != before.scene;
            changes.borrow_mut().record(before, &updated);
            *state.borrow_mut() = updated;
            w.set_editor_can_undo(changes.borrow().can_undo());
            w.set_editor_can_redo(changes.borrow().can_redo());
            refresh_layers(&w, &state.borrow(), index);
            if drag && changed {
                if w.get_active() {
                    w.invoke_start_preview();
                } else {
                    w.set_status("位置を変更しました。入力を適用すると映像へ反映します。".into());
                }
            }
        };
        if drag {
            w.on_layer_dragged(move || edit(false));
        } else {
            w.on_layer_lock(edit);
        }
    }
    for operation in 0..5 {
        let changes = history.clone();
        let weak = w.as_weak();
        let state = scene.clone();
        let callback = move |direction: i32| {
            let Some(w) = weak.upgrade() else {
                return;
            };
            if w.get_recording() || w.get_streaming() || w.get_busy() {
                return;
            }
            let result = (|| -> Result<(), String> {
                let mut scene = state.borrow_mut();
                let mut index = w.get_layer_index();
                let before = crate::scene_history::Snapshot {
                    scene: scene.clone(),
                    selected: index,
                };
                match operation {
                    0 => {
                        if scene.layers.len() >= 8 {
                            return Err("レイヤーは最大8個です".into());
                        }
                        if w.get_layer_kind() == 0 {
                            #[cfg(windows)]
                            {
                                if let Some(path) = rfd::FileDialog::new()
                                    .add_filter("画像", &["png", "webp", "jpg", "jpeg"])
                                    .pick_file()
                                {
                                    w.set_layer_location(path.to_string_lossy().as_ref().into());
                                } else {
                                    return Ok(());
                                }
                            }
                        }
                        let layer = edited_layer(&w)?;
                        scene.layers.push(layer);
                        index = scene.layers.len() as i32 - 1;
                    }
                    1 => {
                        let layer = edited_layer(&w)?;
                        *scene
                            .layers
                            .get_mut(index as usize)
                            .ok_or("更新するレイヤーを選んでください")? = layer;
                    }
                    2 => {
                        if index >= 0 && (index as usize) < scene.layers.len() {
                            scene.layers.remove(index as usize);
                        }
                    }
                    4 => {
                        let mut layer = scene
                            .layers
                            .get(index as usize)
                            .cloned()
                            .ok_or("複製するレイヤーを選んでください")?;
                        if scene.layers.len() >= 8 {
                            return Err("レイヤーは最大8個です".into());
                        }
                        // Preserve source/settings and place the copy directly in front.
                        // Trim by UTF-8 byte length so long Japanese names remain valid.
                        let suffix = " のコピー";
                        while layer.name.len() + suffix.len() > 200 {
                            layer.name.pop();
                        }
                        layer.name.push_str(suffix);
                        scene.layers.insert(index as usize + 1, layer);
                        index += 1;
                    }
                    _ => {
                        let next = index + direction;
                        if index >= 0 && next >= 0 && (next as usize) < scene.layers.len() {
                            scene.layers.swap(index as usize, next as usize);
                            index = next;
                        }
                    }
                }
                changes.borrow_mut().record(before, &scene);
                w.set_editor_can_undo(changes.borrow().can_undo());
                w.set_editor_can_redo(changes.borrow().can_redo());
                refresh_layers(&w, &scene, index);
                w.set_status(if operation==4 {"複製しました。位置は元と同じです。X・Yを変更して更新し、「入力を適用」で反映してください。"}else{"変更しました。「入力を適用」で映像へ反映します。"}.into());
                Ok(())
            })();
            if let Err(e) = result {
                w.set_status(e.into());
            }
        };
        match operation {
            0 => w.on_layer_add(move || callback(0)),
            1 => w.on_layer_apply(move || callback(0)),
            2 => w.on_layer_remove(move || callback(0)),
            3 => w.on_layer_move(callback),
            _ => w.on_layer_duplicate(move || callback(0)),
        }
    }
    for save in [false, true] {
        let changes = history.clone();
        let weak = w.as_weak();
        let state = scene.clone();
        let callback = move || {
            let Some(w) = weak.upgrade() else {
                return;
            };
            #[cfg(windows)]
            {
                let dialog = rfd::FileDialog::new().add_filter("Stream Smash シーン", &["json"]);
                let path = if save {
                    dialog.set_file_name("scene.json").save_file()
                } else {
                    dialog.pick_file()
                };
                if let Some(path) = path {
                    let result = if save {
                        state.borrow().save(&path)
                    } else {
                        streamsmash_media::scene::Scene::load(&path).map(|s| {
                            let before = crate::scene_history::Snapshot {
                                scene: state.borrow().clone(),
                                selected: w.get_layer_index(),
                            };
                            changes.borrow_mut().record(before, &s);
                            w.set_editor_can_undo(changes.borrow().can_undo());
                            w.set_editor_can_redo(changes.borrow().can_redo());
                            *state.borrow_mut() = s;
                            refresh_layers(&w, &state.borrow(), 0);
                        })
                    };
                    w.set_status(match result {
                        Ok(()) => {
                            "シーンを保存／読み込みました。変更後は入力を適用してください。".into()
                        }
                        Err(e) => e.into(),
                    });
                }
            }
            #[cfg(not(windows))]
            {
                let _ = (&state, save);
                w.set_status("このプロトタイプのシーン操作はWindows版で利用できます".into());
            }
        };
        if save {
            w.on_scene_save(callback);
        } else {
            w.on_scene_load(callback);
        }
    }
    let weak = w.as_weak();
    let commands = controller.commands.clone();
    w.on_start_stream(move || {
        let Some(w) = weak.upgrade() else {
            return;
        };
        if w.get_encoder_probing() {
            return;
        }
        let result = config(&w, &devices.borrow(), &scene.borrow()).and_then(|config| {
            streamsmash_media::streaming::StreamTarget::new(
                w.get_stream_server().as_str(),
                w.get_stream_key().to_string(),
                w.get_bitrate() as u32,
            )
            .map(|target| (config, target))
        });
        match result {
            Ok((config, target)) => {
                w.set_stream_key("".into());
                w.set_busy(true);
                w.set_status("RTMPSへ接続しています…".into());
                let _ = commands.send(Control::Stream {
                    config,
                    target,
                    record: w.get_stream_record(),
                });
            }
            Err(e) => w.set_status(e.into()),
        }
    });
    let commands = controller.commands.clone();
    w.on_save_replay(move || {
        let _ = commands.send(Control::SaveReplay);
    });
    let commands = controller.commands.clone();
    w.on_timer_action(move |index| {
        use streamsmash_media::timer::TimerAction;
        let _ = commands.send(Control::Timer(match index {
            0 => TimerAction::Start,
            1 => TimerAction::Pause,
            _ => TimerAction::Reset,
        }));
    });
}

fn save_replay_preferences(window: &MainWindow, path: &std::path::Path) -> Result<(), String> {
    crate::output_settings::save_playback(
        path,
        window.get_highlight_autoplay(),
        window.get_highlight_delay() as u32,
        window.get_replay_tv_overlay(),
        window.get_highlight_post_roll() as u32,
    )?;
    crate::output_settings::save_recognition(
        path,
        window.get_highlight_seconds() as u32,
        window.get_auto_detection(),
        window.get_auto_repeat(),
    )
}

fn setup_auto_replay(
    window: &MainWindow,
    controller: Rc<Controller>,
    engine: Rc<RefCell<Option<crate::auto_replay::Engine>>>,
    bridge: Rc<RefCell<Option<streamsmash_media::replaylab::Bridge>>>,
) {
    let weak = window.as_weak();
    window.on_auto_action(move |index| {
        let Some(w) = weak.upgrade() else {
            return;
        };
        if index == 17 {
            if !w.get_buffering() || w.get_recording() || w.get_streaming() || w.get_busy() { return; }
            let _ = controller.commands.send(Control::CancelHighlights);
            *engine.borrow_mut() = None;
            *bridge.borrow_mut() = None;
            w.set_highlight_playing(false); w.set_auto_monitoring(false); w.set_auto_busy(false);
            w.set_replay_enabled(false);
            w.invoke_start_preview();
            w.set_auto_status("リプレイの認識・蓄積・再生待機を停止します。入力をつなぎ直して通常プレビューへ戻します。".into());
            return;
        }
        let index=if (14..=16).contains(&index) {
            if w.get_auto_busy() {return;}
            w.set_highlight_autoplay(index!=16);
            w.set_highlight_delay(if index==15 {8}else{5});
            w.set_highlight_post_roll(if index==15 {3}else{1});
            12
        }else{index};
        if index == 12 || index == 13 {
            let _=controller.commands.send(Control::SetReplayOverlay(w.get_replay_tv_overlay()));
            let result=std::env::current_exe().map_err(|e|e.to_string()).and_then(|exe|save_replay_preferences(&w,&exe.with_file_name("output-settings.json")));
            w.set_auto_status(match result{Ok(())=>if index==13 {"TV風REPLAY表示を配信映像へ反映しました".into()}else{"設定を保存しました。次の認識STARTから使います".into()},Err(e)=>e.into()});return;
        }
        if index == 11 {
            if let Some(engine)=engine.borrow_mut().as_mut(){engine.play_now();}return;
        }
        if index == 6 {
            let _ = controller.commands.send(Control::CancelHighlights);
            *engine.borrow_mut() = None;
            *bridge.borrow_mut() = None;
            w.set_highlight_playing(false);
            w.set_auto_monitoring(false);
            w.set_auto_busy(false);
            w.set_auto_status("認識だけ停止しました。リプレイの蓄積・再生待機は継続中です。負荷も止める場合は「リプレイ全停止」を使ってください。".into());
            return;
        }
        if index == 7 {
            w.set_highlight_count(0);
            let _ = controller.commands.send(Control::CancelHighlights);
            if let Some(engine) = engine.borrow_mut().as_mut() {
                engine.arm();
            }
            return;
        }
        if index == 9 {
            if let Some(engine) = engine.borrow_mut().as_mut() {
                engine.cancel_playlist();
            }
            let _ = controller.commands.send(Control::CancelHighlights);
            w.set_auto_status("リプレイ再生を中止し、ライブ映像へ戻しました".into());
            return;
        }
        if index == 8 {
            let result = (|| -> Result<PathBuf, String> {
                let root = if let Some(root) = std::env::var_os("SMASHREPLAYLAB_DATA_DIR") {
                    PathBuf::from(root)
                } else {
                    PathBuf::from(
                        std::env::var_os("LOCALAPPDATA")
                            .ok_or("学習データの保存先を取得できません")?,
                    )
                    .join("SmashReplayLab")
                };
                let folder = root.join(if w.get_auto_training() == 1 {
                    "stock_samples"
                } else {
                    "calibration_samples"
                });
                for category in ["positive", "negative"] {
                    std::fs::create_dir_all(folder.join(category))
                        .map_err(|e| format!("学習フォルダーを作成できません: {e}"))?;
                }
                crate::folders::open(&folder)?;
                Ok(folder)
            })();
            w.set_auto_status(
                match result {
                    Ok(folder) => format!("学習フォルダー: {}", folder.display()),
                    Err(error) => error,
                }
                .into(),
            );
            return;
        }
        if engine.borrow().is_some() {
            return;
        }
        if index == 5 && (!w.get_active() || w.get_busy()) {
            w.set_auto_status("入力の準備ができていません。プレビューの映像が動き、入力の適用が完了してからSTARTしてください。入力エラーの場合は画面下部の詳細を確認してください。".into());
            return;
        }
        if index == 5 && !w.get_buffering() {
            w.set_auto_status("リプレイを有効化してプレビューを開始してください".into());
            return;
        }
        if index == 5
            && w.get_auto_detection() != 0
            && w.get_highlight_seconds() > w.get_replay_seconds()
        {
            w.set_auto_status("撃墜クリップの秒数をリプレイの蓄積秒数以下にしてください".into());
            return;
        }
        let action = match index {
            1 => "positive",
            2 => "negative",
            3 => "collect",
            4 => "calibrate",
            5 => "monitor",
            _ => "info",
        };
        let result = (|| -> Result<crate::auto_replay::Engine, String> {
            let exe = std::env::current_exe().map_err(|e| e.to_string())?;
            let folder = exe.parent().ok_or("実行フォルダーを取得できません")?;
            if index==5 {save_replay_preferences(&w,&folder.join("output-settings.json"))?;let _=controller.commands.send(Control::SetReplayOverlay(w.get_replay_tv_overlay()));}
            let connection = folder.join("auto-replay-connection.json");
            if matches!(index, 1 | 2 | 3 | 5) && bridge.borrow().is_none() {
                *bridge.borrow_mut() = Some(streamsmash_media::replaylab::Bridge::start_with_frames(
                    &connection,
                    controller.commands.clone(),
                    controller.recognition_frames.clone(),
                )?);
            }
            crate::auto_replay::Engine::start(
                folder,
                action,
                &connection,
                crate::auto_replay::Options {
                    region: [
                        w.get_auto_x(),
                        w.get_auto_y(),
                        w.get_auto_width(),
                        w.get_auto_height(),
                    ],
                    dry: w.get_auto_dry(),
                    automatic: w.get_auto_repeat(),
                    profile: if index == 5 {
                        match w.get_auto_detection() {
                            1 => "stock",
                            2 => "both",
                            _ => "gameset",
                        }
                    } else if w.get_auto_training() == 1 {
                        "stock"
                    } else {
                        "gameset"
                    },
                    highlight_seconds: w.get_highlight_seconds() as u32,
                    playback_delay: w.get_highlight_delay() as u32,
                    post_roll: w.get_highlight_post_roll() as u32,
                    autoplay: w.get_highlight_autoplay(),
                },
            )
        })();
        match result {
            Ok(task) => {
                *engine.borrow_mut() = Some(task);
                w.set_auto_busy(true);
                w.set_auto_monitoring(index == 5);
                w.set_auto_status("認識エンジンを準備しています…".into());
            }
            Err(e) => {
                *bridge.borrow_mut() = None;
                w.set_auto_status(e.into());
            }
        }
    });
}

pub fn run(smoke: bool) -> Result<(), Box<dyn std::error::Error>> {
    let window = MainWindow::new()?;
    let output_window = Rc::new(RefCell::new(None::<OutputWindow>));
    let output = output_window.clone();
    let weak = window.as_weak();
    let diagnostic_window = window.as_weak();
    window.on_diagnose_camera(move||{
        if let Some(w)=diagnostic_window.upgrade(){
            #[cfg(windows)]{
                let data=streamsmash_media::virtual_camera::diagnostics();
                let saved=std::env::current_exe().map_err(|e|e.to_string()).and_then(|p|{let folder=p.with_file_name("diagnostics");std::fs::create_dir_all(&folder).map_err(|e|e.to_string())?;let file=folder.join("camera-status.json");std::fs::write(&file,serde_json::to_vec_pretty(&data).unwrap()).map_err(|e|e.to_string())?;Ok(file)});
                let message=if let Some(error)=data["error"].as_str(){error.to_string()}else{format!("DLL確認OK：{} / 登録済み：{}。プレビューを開始し、DiscordでStreamSmash Cameraを選んでください",data["selected_dll"].as_str().unwrap_or_default(),data["registered_file_exists"])};
                w.set_status(format!("{message} / {}",match saved{Ok(p)=>format!("診断保存：{}",p.display()),Err(e)=>format!("診断を保存できません：{e}")}).into());
            }
            #[cfg(not(windows))]w.set_status("仮想カメラはWindows版のみ対応しています".into());
        }
    });
    window.on_show_output(move || {
        let result = (|| -> Result<(), slint::PlatformError> {
            if output.borrow().is_none() {
                *output.borrow_mut() = Some(OutputWindow::new()?);
            }
            if let Some(out) = output.borrow().as_ref() {
                if let Some(w) = weak.upgrade() {
                    out.set_frame(w.get_preview());
                }
                out.show()?;
            }
            Ok(())
        })();
        if let Err(e) = result {
            if let Some(w) = weak.upgrade() {
                w.set_status(e.to_string().into());
            }
        }
    });
    let weak = window.as_weak();
    window.on_install_camera(move || {if let Some(w)=weak.upgrade(){
        #[cfg(not(windows))] w.set_status("仮想カメラ登録は現在Windows版のみ対応です".into());
        #[cfg(windows)] {let script=std::env::current_exe().ok().and_then(|p|p.parent().map(|p|p.join("Install-Camera.ps1")));
        if let Some(script)=script.filter(|p|p.is_file()) {use std::os::windows::process::CommandExt;
            let result=std::process::Command::new("powershell.exe").args(["-NoProfile","-ExecutionPolicy","Bypass","-File"]).arg(script).creation_flags(0x08000000).spawn();
            w.set_status(match result{Ok(_)=>"Windowsの確認画面で登録を許可してください。成功後、Discordを起動し直してStreamSmash Cameraを選択します。".into(),Err(e)=>e.to_string().into()});
        }else{w.set_status("配布フォルダーのInstall-Camera.ps1から登録してください".into());}}
    }});
    let folder = std::env::current_exe()?
        .parent()
        .ok_or("Executable has no parent")?
        .join("Recordings");
    window.set_folder(folder.to_string_lossy().as_ref().into());
    let output_settings = folder
        .parent()
        .ok_or("保存先設定のフォルダーを取得できません")?
        .join("output-settings.json");
    let (autoplay, delay, replay_overlay, post_roll) =
        crate::output_settings::playback(&output_settings);
    window.set_replay_tv_overlay(replay_overlay);
    window.set_highlight_autoplay(autoplay);
    window.set_highlight_delay(delay as i32);
    window.set_highlight_post_roll(post_roll as i32);
    let (clip_seconds, profile, repeat) = crate::output_settings::recognition(&output_settings);
    window.set_highlight_seconds(clip_seconds as i32);
    window.set_auto_detection(profile);
    window.set_auto_repeat(repeat);
    let replay_folder = match crate::output_settings::load(&output_settings) {
        Ok(Some(path)) => path,
        Ok(None) => folder.clone(),
        Err(e) => {
            window.set_auto_status(e.into());
            folder.clone()
        }
    };
    window.set_replay_folder(replay_folder.to_string_lossy().as_ref().into());
    if let Ok(settings) = std::fs::read_to_string(audio_settings_path()) {
        for line in settings.lines() {
            if let Some((key, value)) = line.split_once('=') {
                if let Ok(value) = value.parse::<i32>() {
                    match key {
                        "format" if (0..=1).contains(&value) => {
                            window.set_audio_format_index(value)
                        }
                        "offset_ms" if (-3000..=3000).contains(&value) => {
                            window.set_audio_offset_ms(value)
                        }
                        _ => {}
                    }
                }
            }
        }
    }
    setup_preferences_at(
        &window,
        std::env::current_exe()?.with_file_name("studio-settings.json"),
    );
    window.set_sources(strings([
        "画面キャプチャー（高速 / DXGI）".into(),
        "画面キャプチャー（互換 / 全画面）".into(),
    ]));
    window.set_audio_sources(strings([
        "PCで再生している音（ゲーム・動画）".into(),
        "音声なし".into(),
    ]));
    let controller = Rc::new(Controller::new());
    let _ = controller
        .commands
        .send(Control::SetReplayOverlay(replay_overlay));
    let _ = controller
        .commands
        .send(Control::SetReplayFolder(replay_folder));
    let weak = window.as_weak();
    let settings = output_settings.clone();
    let commands = controller.commands.clone();
    window.on_apply_replay_folder(move || {
        if let Some(w) = weak.upgrade() {
            let result = (|| -> Result<(), String> {
                let value = w.get_replay_folder();
                if value.trim().is_empty() {
                    return Err("リプレイ動画の保存先を選んでください".into());
                }
                let folder =
                    crate::output_settings::apply(&settings, &PathBuf::from(value.trim()))?;
                commands
                    .send(Control::SetReplayFolder(folder))
                    .map_err(|_| "保存処理に接続できません".to_string())?;
                Ok(())
            })();
            if let Err(e) = result {
                w.set_auto_status(format!("保存先を設定できません: {e}").into());
            }
        }
    });
    let weak = window.as_weak();
    window.on_choose_record_folder(move || {
        let Some(w) = weak.upgrade() else {
            return;
        };
        if w.get_recording() || w.get_streaming() || w.get_busy() || w.get_record_folder_picking() {
            return;
        }
        #[cfg(windows)]
        {
            w.set_record_folder_picking(true);
            let initial = PathBuf::from(w.get_folder().as_str());
            let weak = weak.clone();
            std::thread::spawn(move || {
                let folder = rfd::FileDialog::new()
                    .set_title("録画の保存先")
                    .set_directory(initial)
                    .pick_folder();
                let _ = weak.upgrade_in_event_loop(move |w| {
                    w.set_record_folder_picking(false);
                    if let Some(folder) = folder {
                        if w.get_recording() || w.get_streaming() || w.get_busy() {
                            w.set_status(
                                "保存先の変更は録画・配信を停止してから行ってください".into(),
                            );
                        } else {
                            w.set_folder(folder.to_string_lossy().as_ref().into());
                            w.set_status(
                                "この起動中の録画保存先を変更しました。次の録画から使われます"
                                    .into(),
                            );
                        }
                    }
                });
            });
        }
        #[cfg(not(windows))]
        w.set_status("録画の保存先を入力してください".into());
    });
    let weak = window.as_weak();
    window.on_choose_replay_folder(move || {
        let Some(w) = weak.upgrade() else {
            return;
        };
        #[cfg(windows)]
        {
            if w.get_replay_folder_picking() {
                return;
            }
            w.set_replay_folder_picking(true);
            let initial = PathBuf::from(w.get_replay_folder().as_str());
            let weak = weak.clone();
            std::thread::spawn(move || {
                let folder = rfd::FileDialog::new()
                    .set_title("リプレイ動画の保存先")
                    .set_directory(initial)
                    .pick_folder();
                let _ = weak.upgrade_in_event_loop(move |w| {
                    w.set_replay_folder_picking(false);
                    if let Some(folder) = folder {
                        w.set_replay_folder(folder.to_string_lossy().as_ref().into());
                        w.invoke_apply_replay_folder();
                    }
                });
            });
        }
        #[cfg(not(windows))]
        w.set_auto_status("保存先を入力して適用してください".into());
    });
    let weak = window.as_weak();
    window.on_open_replay_folder(move || {
        if let Some(w) = weak.upgrade() {
            if let Err(e) = crate::folders::open(&PathBuf::from(w.get_replay_folder().as_str())) {
                w.set_auto_status(e.into());
            }
        }
    });
    let phone = crate::phone_remote::setup(&window);
    let auto_engine = Rc::new(RefCell::new(None::<crate::auto_replay::Engine>));
    let auto_bridge = Rc::new(RefCell::new(None::<streamsmash_media::replaylab::Bridge>));
    setup_auto_replay(
        &window,
        controller.clone(),
        auto_engine.clone(),
        auto_bridge.clone(),
    );
    if !smoke {
        window.invoke_auto_action(0);
    }
    let replaylab = Rc::new(RefCell::new(None::<streamsmash_media::replaylab::Bridge>));
    let bridge = replaylab.clone();
    let bridge_commands = controller.commands.clone();
    let bridge_frames = controller.recognition_frames.clone();
    let weak = window.as_weak();
    window.on_toggle_replaylab(move |enabled| {
        if let Some(w) = weak.upgrade() {
            if enabled {
                let result = std::env::current_exe()
                    .map_err(|e| e.to_string())
                    .and_then(|exe| {
                        let path = exe
                            .parent()
                            .ok_or("実行フォルダーを取得できません")?
                            .join("replaylab-connection.json");
                        streamsmash_media::replaylab::Bridge::start_with_frames(
                            &path,
                            bridge_commands.clone(),
                            bridge_frames.clone(),
                        )
                        .map(|b| (b, path))
                    });
                match result {
                    Ok((b, path)) => {
                        *bridge.borrow_mut() = Some(b);
                        w.set_replaylab_status(format!("接続ファイル: {}", path.display()).into());
                    }
                    Err(e) => {
                        w.set_replaylab_enabled(false);
                        w.set_replaylab_status(format!("連携を開始できません: {e}").into());
                    }
                }
            } else {
                *bridge.borrow_mut() = None;
                w.set_replaylab_status("SmashReplayLab連携: OFF".into());
            }
        }
    });
    let weak = window.as_weak();
    window.on_launch_replaylab(move || {
        if let Some(w) = weak.upgrade() {
            let result = (|| -> Result<(), String> {
                let exe = std::env::current_exe().map_err(|e| e.to_string())?;
                let folder = exe.parent().ok_or("実行フォルダーを取得できません")?;
                let lab = folder.join(if cfg!(windows) {
                    "SmashReplayLab.exe"
                } else {
                    "SmashReplayLab"
                });
                if !lab.is_file() {
                    return Err("同じフォルダーに更新版SmashReplayLab.exeを置いてください".into());
                }
                let mut command = std::process::Command::new(lab);
                command
                    .arg("--connection-file")
                    .arg(folder.join("replaylab-connection.json"));
                #[cfg(windows)]
                {
                    use std::os::windows::process::CommandExt;
                    command.creation_flags(0x08000000);
                }
                command.spawn().map_err(|e| e.to_string())?;
                Ok(())
            })();
            if let Err(e) = result {
                w.set_status(e.into());
            }
        }
    });
    let outputs = output_window.clone();
    window.window().on_close_requested(move || {
        if let Some(out) = outputs.borrow().as_ref() {
            let _ = out.hide();
        }
        slint::CloseRequestResponse::HideWindow
    });
    let devices = Rc::new(RefCell::new(Devices::default()));
    let scene = Rc::new(RefCell::new(streamsmash_media::scene::Scene::default()));
    setup_studio(&window, scene.clone(), devices.clone(), controller.clone());
    for record in [false, true] {
        let weak = window.as_weak();
        let controller = controller.clone();
        let devices = devices.clone();
        let scene = scene.clone();
        let callback = move || {
            if let Some(window) = weak.upgrade() {
                if window.get_encoder_probing() {
                    return;
                }
                match config(&window, &devices.borrow(), &scene.borrow()) {
                    Ok(config) => {
                        let _ = std::fs::write(
                            audio_settings_path(),
                            format!(
                                "format={}\noffset_ms={}\n",
                                window.get_audio_format_index(),
                                window.get_audio_offset_ms()
                            ),
                        );
                        window.set_busy(true);
                        window.set_status(
                            if record {
                                "録画を開始しています…"
                            } else {
                                "入力を開いています…"
                            }
                            .into(),
                        );
                        let _ = controller.commands.send(if record {
                            Control::Record(config)
                        } else {
                            Control::Preview(config)
                        });
                    }
                    Err(e) => window.set_status(e.into()),
                }
            }
        };
        if record {
            window.on_start_recording(callback);
        } else {
            window.on_start_preview(callback);
        }
    }
    let commands = controller.commands.clone();
    window.on_stop(move || {
        let _ = commands.send(Control::Stop);
    });
    let commands = controller.commands.clone();
    let weak = window.as_weak();
    window.on_refresh(move || {
        if let Some(window) = weak.upgrade() {
            window.set_status("接続デバイスを取得しています…".into());
        }
        let _ = commands.send(Control::Refresh);
    });
    let weak = window.as_weak();
    window.on_open_folder(move || {
        if let Some(window) = weak.upgrade() {
            let path = PathBuf::from(window.get_folder().as_str());
            if let Err(e) = crate::folders::open(&path) {
                window.set_status(e.into());
            }
        }
    });
    let commands = controller.commands.clone();
    if !smoke {
        let _ = commands.send(Control::Refresh);
    }
    let weak = window.as_weak();
    let frames = Rc::new(std::cell::Cell::new(0_u32));
    let count = frames.clone();
    let start = std::time::Instant::now();
    let mut test = TestPattern::new(VideoFormat {
        width: 1920,
        height: 1080,
        fps: 30,
    })?;
    let mut recording_since = None;
    let encoder_probe = setup_encoder_probe(&window);
    let mut performance_at = std::time::Instant::now();
    let mut performance_input = 0_u64;
    let mut performance_display = 0_u32;
    let timer = slint::Timer::default();
    timer.start(slint::TimerMode::Repeated, Duration::from_millis(33), move || {
        let Some(window) = weak.upgrade() else { return; };
        while let Ok(reply)=encoder_probe.results.try_recv(){apply_encoder_probe_reply(&window,reply);}
        while let Ok(event) = controller.events.try_recv() {
            if let Event::Error(error)=&event {
                // The session logger has already redacted the stream key.
                if let Ok(exe)=std::env::current_exe() {
                    if let Some(folder)=exe.parent() {
                        let diagnostics=folder.join("diagnostics");
                        if std::fs::create_dir_all(&diagnostics).is_ok(){
                            let _=std::fs::write(diagnostics.join("last-input-error.txt"),format!("Stream Smash 0.20.4\n{error}\n"));
                        }
                    }
                }
            }
            if window.get_auto_monitoring() {
                if let Some(reason)=crate::auto_replay::input_stop_reason(&event) {
                    window.set_highlight_playing(false);
                    *auto_engine.borrow_mut()=None;
                    *auto_bridge.borrow_mut()=None;
                    window.set_auto_monitoring(false);window.set_auto_busy(false);
                    window.set_auto_status(reason.into());
                }
            }
            match event {
                Event::Devices(found) => {
                    refresh_microphones(&window,&devices.borrow(),&found);
                    window.set_sources(strings(["画面キャプチャー（高速 / DXGI）".into(),"画面キャプチャー（互換 / 全画面）".into()].into_iter().chain(found.video.iter().map(|d| d.name.clone()))));
                    window.set_audio_sources(strings(["PCで再生している音（ゲーム・動画）".into(),"音声なし".into()].into_iter().chain(found.audio.iter().map(|d| d.name.clone()))));
                    window.set_source_index(0); window.set_audio_index(0);
                    window.set_status(format!("映像デバイス {} 台・音声入力 {} 台。入力を選択して開始してください。",found.video.len(),found.audio.len()).into());
                    *devices.borrow_mut() = found;
                }
                Event::Starting => { window.set_busy(true); }
                Event::Started { recording, streaming, path } => {
                    window.set_busy(false); window.set_active(true); window.set_recording(recording); window.set_streaming(streaming); window.set_buffering(window.get_replay_enabled());
                    recording_since = if recording || streaming { Some(std::time::Instant::now()) } else { None };
                    window.set_status(if streaming {"RTMPSへ送信中。YouTube Studioで受信状態を確認してください。".into()} else {match path { Some(path) => format!("録画中：{}",path.display()),None => "プレビュー中。録画開始で選択した形式へ保存します。".into() }}.into());
                }
                Event::Stopped { saved } => {
                    window.set_busy(false); window.set_active(false); window.set_recording(false); window.set_streaming(false); window.set_buffering(false);
                    recording_since = None;
                    window.set_status(match saved { Some(path) => format!("保存しました：{}",path.display()), None => "停止しました".into() }.into());
                }
                Event::Error(e) => {
                    window.set_busy(false); window.set_active(false); window.set_recording(false); window.set_streaming(false); window.set_buffering(false);
                    recording_since = None;
                    window.set_status(format!("エラー：{e}").into());
                }
                Event::HighlightsStarted => {
                    window.set_highlight_playing(true);
                    if let Some(engine)=auto_engine.borrow_mut().as_mut(){engine.playback_state(true);}
                    window.set_auto_status("バースト集をプレビュー・録画・配信へ再生中。終了後にライブへ戻ります".into());
                },
                Event::HighlightsFinished => {
                    window.set_highlight_playing(false);
                    if let Some(engine)=auto_engine.borrow_mut().as_mut(){engine.playback_state(false);}
                    window.set_auto_status(if window.get_auto_repeat(){"ライブ復帰。次の試合の認識を待ちます"}else{"ライブ復帰。次の試合は「次の試合へ」を押してください"}.into());
                },
                Event::HighlightsFailed(error) => {
                    window.set_highlight_playing(false);
                    if let Some(engine)=auto_engine.borrow_mut().as_mut(){engine.playback_state(false);}
                    window.set_auto_status(format!("リプレイを再生できません：{error}").into());
                },
                Event::ReplaySaved(path) => window.set_status(format!("リプレイを保存しました：{}",path.display()).into()),
                Event::ReplayFolderChanged(path) => {
                    let path=crate::folders::normalized(&path);
                    window.set_replay_folder(path.to_string_lossy().as_ref().into());
                    window.set_auto_status(format!("次のリプレイ保存からこのフォルダーを使用します：{}",path.display()).into());
                }
                Event::Warning(e) => window.set_status(format!("注意：{e}").into()),
            }
        }
        let mut auto_finished = false;
        if let Some(engine) = auto_engine.borrow_mut().as_mut() {
            auto_finished = engine.finished();
            while let Ok(event) = engine.events.try_recv() {
                match event["kind"].as_str().unwrap_or_default() {
                    "learning" => {
                        if (event["profile"].as_str()==Some("stock")) != (window.get_auto_training()==1) {continue;}
                        if let Some(region)=event["region"].as_array().filter(|r|r.len()==4) {
                            window.set_auto_x(region[0].as_i64().unwrap_or(130) as i32);window.set_auto_y(region[1].as_i64().unwrap_or(50) as i32);
                            window.set_auto_width(region[2].as_i64().unwrap_or(380) as i32);window.set_auto_height(region[3].as_i64().unwrap_or(195) as i32);
                        }
                        window.set_auto_learning(format!("{} {}枚 / 通常 {}枚：{}",if window.get_auto_training()==1 {"－表示"} else {"GAME SET"},event["positives"],event["negatives"],event["message"].as_str().unwrap_or_default()).into());
                        window.set_auto_status(event["message"].as_str().unwrap_or_default().into());
                    }
                    "clips" => {
                        if !engine.arm_pending {window.set_highlight_count(event["paths"].as_array().map_or(0,|p|p.len()) as i32);}
                    }
                    "playlist" => {
                        if engine.arm_pending {continue;}
                        window.set_auto_status(event["message"].as_str().unwrap_or_default().into());
                        if let Some(paths)=event["paths"].as_array().filter(|a|!a.is_empty()){
                            let paths=paths.iter().filter_map(|p|p.as_str().map(PathBuf::from)).collect();
                            let _=controller.commands.send(Control::PlayHighlightsAfter {paths,lead_ms:event["lead_ms"].as_u64().unwrap_or(1000).clamp(1000,120000) as u32});
                        }
                    }
                    "timing" => {
                        use std::io::Write;
                        if let Ok(exe)=std::env::current_exe() {
                            let folder=exe.with_file_name("diagnostics");let _=std::fs::create_dir_all(&folder);
                            let path=folder.join("replay-timing.jsonl");
                            if std::fs::metadata(&path).is_ok_and(|m|m.len()>1048576) {let _=std::fs::rename(&path,folder.join("replay-timing.previous.jsonl"));}
                            if let Ok(mut f)=std::fs::OpenOptions::new().create(true).append(true).open(path) {let _=writeln!(f,"{event}");}
                        }
                    }
                    "match_started" => {engine.arm_pending=false;window.set_auto_status(event["message"].as_str().unwrap_or_default().into());},
                    "score" => window.set_auto_score(event["summary"].as_str().unwrap_or_default().into()),
                    "message" | "match_wait" | "detected" | "saved" | "error" => window.set_auto_status(event["message"].as_str().unwrap_or_default().into()),
                    _ => {}
                }
            }
        }
        if auto_finished {
            *auto_engine.borrow_mut() = None;
            *auto_bridge.borrow_mut() = None;
            window.set_auto_monitoring(false);window.set_auto_busy(false);
            if window.get_auto_status().as_str() == "認識エンジンを準備しています…" {
                window.set_auto_status("認識エンジンが終了しました。tools/replay-detectorの配置を確認してください。".into());
            }
        }
        window.set_camera_status(match controller.camera_state.load(std::sync::atomic::Ordering::Relaxed){1=>"仮想カメラ: 720p30送出中・接続待ち",2=>"仮想カメラ: 720p30送出中・アプリ接続あり",3=>"仮想カメラ: エラー（下部の詳細・カメラ診断を確認）",_=>"仮想カメラ: OFF"}.into());
        let ms=controller.timer.milliseconds();
        window.set_race_time(format!("{:02}:{:02}:{:02}.{:02}",ms/3600000,ms/60000%60,ms/1000%60,ms/10%100).into());
        if let Some(since) = recording_since { let s = since.elapsed().as_secs(); window.set_elapsed(format!("{:02}:{:02}",s/60,s%60).into()); }
        phone.borrow_mut().tick(&window,&controller);
        let frame = if smoke { test.next_frame(start.elapsed()).ok().map(|f| PreviewFrame { width: f.format.width, height: f.format.height, rgba: f.rgba }) } else { controller.latest.lock().unwrap().take() };
        if let Some(frame) = frame {
            if window.get_active() && !window.get_busy(){phone.borrow().frame(&frame);}
            let buffer = slint::SharedPixelBuffer::<slint::Rgba8Pixel>::clone_from_slice(&frame.rgba,frame.width,frame.height);
            let image=slint::Image::from_rgba8(buffer);
            if let Some(out)=output_window.borrow().as_ref(){out.set_frame(image.clone());}
            window.set_preview(image);
            window.set_has_image(true);
            count.set(count.get()+1);
        }
        if performance_at.elapsed() >= Duration::from_secs(2) {
            let seconds = performance_at.elapsed().as_secs_f64();
            let input = controller.recognition_frames.input_count();
            let display = count.get();
            window.set_performance_note(format!("受信 {:.1} / 表示 {:.1} fps", input.saturating_sub(performance_input) as f64 / seconds, display.wrapping_sub(performance_display) as f64 / seconds).into());
            performance_input = input; performance_display = display; performance_at = std::time::Instant::now();
        }
    });
    let quit = slint::Timer::default();
    if smoke {
        quit.start(slint::TimerMode::SingleShot, Duration::from_secs(2), || {
            let _ = slint::quit_event_loop();
        });
    }
    window.run()?;
    drop(timer);
    let _ = commands.send(Control::Quit);
    if smoke {
        if frames.get() == 0 {
            return Err("No preview frames rendered".into());
        }
        println!("UI smoke OK: {} preview frames", frames.get());
    }
    Ok(())
}

#[cfg(test)]
mod editor_tests {
    use super::*;
    use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
    use slint::platform::{
        Platform, PlatformError, PointerEventButton, WindowAdapter, WindowEvent,
    };
    use slint::Model;
    struct Offscreen(Rc<MinimalSoftwareWindow>);
    impl Platform for Offscreen {
        fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
            Ok(self.0.clone())
        }
    }
    #[test]
    fn offscreen_drag_respects_locks_and_undo_without_opening_native_windows() {
        use streamsmash_media::scene::{Layer, LayerSource, Scene};
        let surface = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
        slint::platform::set_platform(Box::new(Offscreen(surface.clone()))).unwrap();
        let ui = MainWindow::new().unwrap();
        assert!(
            !ui.get_settings_open(),
            "startup should show the clean workspace"
        );
        assert!(!ui.get_show_timer(), "specialized controls start hidden");
        surface.set_size(slint::PhysicalSize::new(1280, 860));
        let layer = |name: &str| Layer {
            name: name.into(),
            enabled: true,
            locked: false,
            source: LayerSource::Timer,
            x: 100,
            y: 100,
            width: 400,
            height: 240,
            opacity: 1.,
        };
        let state = Rc::new(RefCell::new(Scene {
            version: 1,
            layers: vec![layer("Back"), layer("Front")],
        }));
        setup_studio(
            &ui,
            state.clone(),
            Rc::new(RefCell::new(Devices::default())),
            Rc::new(Controller::new()),
        );
        refresh_layers(&ui, &state.borrow(), 0);
        assert!(ui.get_layers().row_data(0).unwrap().contains("Front"));
        ui.set_studio_page(1);
        ui.set_has_image(true);
        ui.set_expanded(false);
        ui.show().unwrap();
        let mut pixels = vec![slint::Rgb8Pixel::default(); 1280 * 860];
        surface.draw_if_needed(|r| {
            r.render(&mut pixels, 1280);
        });
        let points: Vec<_> = pixels
            .iter()
            .enumerate()
            .filter(|(_, p)| p.r == 54 && p.g == 217 && p.b == 255)
            .map(|(i, _)| ((i % 1280) as f32, (i / 1280) as f32))
            .collect();
        assert!(!points.is_empty(), "selection outline was not rendered");
        let left = points.iter().map(|p| p.0).reduce(f32::min).unwrap();
        let top = points.iter().map(|p| p.1).reduce(f32::min).unwrap();
        let right = points.iter().map(|p| p.0).reduce(f32::max).unwrap();
        let bottom = points.iter().map(|p| p.1).reduce(f32::max).unwrap();
        let x = (left + right) / 2.;
        let y = (top + bottom) / 2.;
        let drag = |x, y| {
            let position = slint::LogicalPosition::new(x, y);
            ui.window()
                .dispatch_event(WindowEvent::PointerMoved { position });
            ui.window().dispatch_event(WindowEvent::PointerPressed {
                position,
                button: PointerEventButton::Left,
            });
            let position = slint::LogicalPosition::new(x + 50., y + 25.);
            ui.window()
                .dispatch_event(WindowEvent::PointerMoved { position });
            ui.window().dispatch_event(WindowEvent::PointerReleased {
                position,
                button: PointerEventButton::Left,
            });
        };
        let applied = Rc::new(std::cell::Cell::new(0));
        let count = applied.clone();
        ui.on_start_preview(move || count.set(count.get() + 1));
        ui.set_active(true);
        drag(x, y);
        assert_eq!(applied.get(), 1);
        let moved = state.borrow().layers[0].clone();
        assert!(
            moved.x > 100 && moved.y > 100,
            "drag did not change coordinates: {moved:?}"
        );
        let expected_dx = (50. * 400. / (right - left)).round() as i32;
        let expected_dy = (25. * 240. / (bottom - top)).round() as i32;
        assert!((moved.x - 100 - expected_dx).abs() <= 3);
        assert!((moved.y - 100 - expected_dy).abs() <= 3);
        assert!(ui.get_editor_can_undo());
        ui.invoke_layer_lock(true);
        drag(x + 50., y + 25.);
        assert_eq!(applied.get(), 1);
        assert_eq!(state.borrow().layers[0].x, moved.x);
        assert_eq!(state.borrow().layers[0].y, moved.y);
        ui.invoke_editor_history(false);
        ui.invoke_editor_history(false);
        assert_eq!(state.borrow().layers[0].x, 100);
        assert_eq!(state.borrow().layers[0].y, 100);
        ui.set_expanded(false);
        ui.window().request_redraw();
        surface.draw_if_needed(|r| {
            r.render(&mut pixels, 1280);
        });
        if let Some(path) = std::env::var_os("STREAMSMASH_UI_PREVIEW") {
            let mut ppm = b"P6\n1280 860\n255\n".to_vec();
            for p in &pixels {
                ppm.extend([p.r, p.g, p.b]);
            }
            std::fs::write(path, ppm).unwrap();
        }
        // Click the actual workspace buttons against spies, never a real recorder.
        let recordings = Rc::new(std::cell::Cell::new(0));
        let count = recordings.clone();
        ui.on_start_recording(move || count.set(count.get() + 1));
        let click = |x, y| {
            let position = slint::LogicalPosition::new(x, y);
            ui.window()
                .dispatch_event(WindowEvent::PointerMoved { position });
            ui.window().dispatch_event(WindowEvent::PointerPressed {
                position,
                button: PointerEventButton::Left,
            });
            ui.window().dispatch_event(WindowEvent::PointerReleased {
                position,
                button: PointerEventButton::Left,
            });
        };
        click(380., 118.);
        assert_eq!(recordings.get(), 1);
        ui.set_busy(true);
        click(380., 118.);
        assert_eq!(recordings.get(), 1, "busy controls must reject clicks");
        ui.set_busy(false);
        click(636., 118.);
        assert!(
            ui.get_settings_open(),
            "empty stream key should lead to setup"
        );
        assert_eq!(ui.get_studio_page(), 2);
        ui.invoke_show_workspace();
        // The toolbar also works while settings or the expanded preview is shown.
        ui.set_expanded(true);
        click(380., 118.);
        assert_eq!(recordings.get(), 2);
        let stopped = Rc::new(std::cell::Cell::new(0));
        let count = stopped.clone();
        ui.on_stop(move || count.set(count.get() + 1));
        click(252., 118.);
        assert_eq!(stopped.get(), 1);
        ui.set_recording(true);
        ui.set_streaming(true);
        click(252., 118.);
        click(508., 118.);
        assert_eq!(
            stopped.get(),
            1,
            "preview/record stop cannot end a shared live stream"
        );
        click(764., 118.);
        assert_eq!(stopped.get(), 2);
        ui.set_recording(false);
        ui.set_streaming(false);
        let sent = Rc::new(std::cell::Cell::new(0));
        let count = sent.clone();
        ui.on_start_stream(move || count.set(count.get() + 1));
        ui.set_stream_key("fixture-only".into());
        click(636., 118.);
        assert_eq!(sent.get(), 1);
        ui.set_stream_key("".into());
        ui.invoke_show_workspace();
        // Exercise plugin host actions against a temporary catalog, without a
        // file picker, native window, browser, network or device capture.
        let scratch =
            std::env::temp_dir().join(format!("streamsmash-plugin-ui-{}", std::process::id()));
        let source = scratch.join("source");
        let store = scratch.join("plugins");
        std::fs::create_dir_all(&source).unwrap();
        std::fs::write(source.join("plugin.json"),r#"{"api_version":1,"id":"ui-fixture","name":"UI fixture","version":"1","kind":"html","entry":"overlay.html"}"#).unwrap();
        std::fs::write(source.join("overlay.html"), "<p>fixture</p>").unwrap();
        crate::plugins::install(&source, &store).unwrap();
        setup_plugins_at(
            &ui,
            state.clone(),
            Rc::new(RefCell::new(crate::scene_history::History::default())),
            store,
        );
        assert_eq!(ui.get_plugin_names().row_count(), 1);
        ui.invoke_plugin_action(3);
        assert_eq!(state.borrow().layers.len(), 3);
        assert!(state.borrow().layers[2].enabled);
        ui.invoke_plugin_action(4);
        assert!(!state.borrow().layers[2].enabled);
        ui.invoke_plugin_action(3);
        assert_eq!(state.borrow().layers.len(), 3);
        assert!(state.borrow().layers[2].enabled);
        ui.invoke_show_settings(5);
        ui.window().request_redraw();
        surface.draw_if_needed(|r| {
            r.render(&mut pixels, 1280);
        });
        if let Some(path) = std::env::var_os("STREAMSMASH_PLUGIN_PREVIEW") {
            let mut ppm = b"P6\n1280 860\n255\n".to_vec();
            for p in &pixels {
                ppm.extend([p.r, p.g, p.b]);
            }
            std::fs::write(path, ppm).unwrap();
        }
        let phone = crate::phone_remote::setup(&ui);
        assert!(phone.borrow().server.is_none());
        assert!(!ui.get_phone_enabled());
        ui.invoke_show_settings(6);
        ui.window().request_redraw();
        surface.draw_if_needed(|r| {
            r.render(&mut pixels, 1280);
        });
        if let Some(path) = std::env::var_os("STREAMSMASH_PHONE_PREVIEW") {
            let mut ppm = b"P6\n1280 860\n255\n".to_vec();
            for p in &pixels {
                ppm.extend([p.r, p.g, p.b]);
            }
            std::fs::write(path, ppm).unwrap();
        }
        ui.invoke_phone_action(1);
        assert!(phone.borrow().server.is_none());
        // Verify menu accelerators use the same guarded callbacks, without opening dialogs.
        let loads = Rc::new(std::cell::Cell::new(0));
        let count = loads.clone();
        ui.on_scene_load(move || count.set(count.get() + 1));
        let saves = Rc::new(std::cell::Cell::new(0));
        let count = saves.clone();
        ui.on_scene_save(move || count.set(count.get() + 1));
        let shortcut = |letter: &str| {
            use slint::platform::Key;
            ui.window().dispatch_event(WindowEvent::KeyPressed {
                text: Key::Control.into(),
            });
            ui.window().dispatch_event(WindowEvent::KeyPressed {
                text: letter.into(),
            });
            ui.window().dispatch_event(WindowEvent::KeyReleased {
                text: letter.into(),
            });
            ui.window().dispatch_event(WindowEvent::KeyReleased {
                text: Key::Control.into(),
            });
        };
        ui.invoke_show_workspace();
        shortcut("o");
        assert_eq!(loads.get(), 1);
        shortcut("s");
        assert_eq!(saves.get(), 1);
        ui.set_recording(true);
        shortcut("o");
        assert_eq!(loads.get(), 1, "loading must be disabled while recording");
        ui.set_recording(false);
        ui.set_sources(strings([
            "画面キャプチャー（高速）".into(),
            "HDMIキャプチャー".into(),
        ]));
        ui.set_audio_sources(strings(["PCで再生している音".into(), "音声なし".into()]));
        let mut pattern = TestPattern::new(VideoFormat {
            width: 640,
            height: 360,
            fps: 30,
        })
        .unwrap();
        let frame = pattern.next_frame(Duration::from_millis(480)).unwrap();
        ui.set_preview(slint::Image::from_rgba8(slint::SharedPixelBuffer::<
            slint::Rgba8Pixel,
        >::clone_from_slice(
            &frame.rgba, 640, 360
        )));
        ui.set_folder("C:\\Users\\Example\\Videos".into());
        ui.set_replay_folder("C:\\Users\\Example\\Videos\\Replays".into());
        ui.set_status("テスト映像です。実際のキャプチャー機器は使っていません。".into());
        let selected_mic = Device {
            name: "テスト用マイク".into(),
            id: r#"[0,"テスト用マイク"]"#.into(),
        };
        let microphone_devices = Devices {
            microphones: vec![selected_mic.clone()],
            ..Default::default()
        };
        refresh_microphones(&ui, &Devices::default(), &microphone_devices);
        ui.set_microphone_enabled(true);
        ui.set_microphone_index(1);
        let capture = config(&ui, &microphone_devices, &Scene::default()).unwrap();
        assert_eq!(capture.microphone, Some(selected_mic.id.clone()));
        refresh_microphones(&ui, &microphone_devices, &Devices::default());
        assert!(
            !ui.get_microphone_enabled(),
            "missing chosen mic must never fall back to another mic"
        );
        assert_eq!(ui.get_microphone_index(), 0);
        let prefs_path = scratch.join("studio-settings.json");
        setup_preferences_at(&ui, prefs_path.clone());
        ui.set_audio_gain(75);
        ui.set_audio_offset_ms(-100);
        ui.set_preview_index(0);
        ui.invoke_settings_preset(2);
        assert_eq!(ui.get_quality_index(), 2);
        assert_eq!(ui.get_audio_gain(), 75);
        assert_eq!(ui.get_audio_offset_ms(), -100);
        assert_eq!(ui.get_preview_index(), 0);
        ui.set_recording(true);
        ui.invoke_settings_preset(0);
        assert_eq!(ui.get_quality_index(), 2);
        assert!(!ui.invoke_save_preferences());
        ui.set_recording(false);
        ui.set_quality_index(3);
        ui.set_custom_width(1919);
        assert!(!ui.invoke_save_preferences());
        assert!(!prefs_path.exists());
        ui.set_custom_width(2560);
        ui.set_custom_height(1440);
        ui.set_custom_fps(60);
        assert!(ui.invoke_save_preferences());
        let saved = crate::preferences::load(&prefs_path).unwrap().unwrap();
        assert_eq!(saved.width, 2560);
        assert_eq!(saved.offset_ms, -100);
        assert_eq!(saved.tuning.gain_percent, 75);
        let source_before = ui.get_source_index();
        let audio_before = ui.get_audio_index();
        let folder_before = ui.get_folder();
        ui.invoke_settings_defaults();
        assert_eq!(ui.get_audio_gain(), 100);
        assert_eq!(ui.get_source_index(), source_before);
        assert_eq!(ui.get_audio_index(), audio_before);
        assert_eq!(ui.get_folder(), folder_before);
        ui.set_custom_width(2560);
        ui.set_custom_height(1440);
        ui.set_custom_fps(60);
        ui.set_status("テスト映像です。実際のキャプチャー機器は使っていません。".into());
        for (width, height) in [(1280, 860), (1040, 720)] {
            surface.set_size(slint::PhysicalSize::new(width, height));
            let mut pixels = vec![slint::Rgb8Pixel::default(); (width * height) as usize];
            for page in -2..=8 {
                ui.set_auto_monitoring(page == -2);
                ui.set_show_timer(page == -2);
                ui.set_highlight_count(if page == -2 { 3 } else { 0 });
                ui.set_auto_status(
                    "自動認識中：GAME SETを検出すると、この試合のリプレイを再生します。".into(),
                );
                if page < 0 {
                    ui.invoke_show_workspace();
                } else {
                    ui.invoke_show_settings(page);
                }
                assert_eq!(ui.get_settings_open(), page >= 0);
                assert!(!ui.get_expanded());
                if page >= 0 {
                    assert_eq!(ui.get_studio_page(), page);
                }
                ui.window().request_redraw();
                surface.draw_if_needed(|r| {
                    r.render(&mut pixels, width as usize);
                });
                if let Some(dir) = std::env::var_os("STREAMSMASH_WORKSPACE_PREVIEWS") {
                    std::fs::create_dir_all(&dir).unwrap();
                    let mut ppm = format!("P6\n{width} {height}\n255\n").into_bytes();
                    for p in &pixels {
                        ppm.extend([p.r, p.g, p.b]);
                    }
                    std::fs::write(
                        std::path::PathBuf::from(dir).join(format!("{width}-page-{page}.ppm")),
                        ppm,
                    )
                    .unwrap();
                }
            }
        }
        for (width, height) in [(1280, 860), (1040, 720)] {
            surface.set_size(slint::PhysicalSize::new(width, height));
            ui.invoke_show_settings(0);
            for tab in 0..3 {
                ui.set_basic_tab(tab);
                std::thread::sleep(Duration::from_millis(180));
                slint::platform::update_timers_and_animations();
                if tab == 1 {
                    ui.set_quality_index(3);
                }
                ui.window().request_redraw();
                let mut pixels = vec![slint::Rgb8Pixel::default(); (width * height) as usize];
                surface.draw_if_needed(|r| {
                    r.render(&mut pixels, width as usize);
                });
                if let Some(dir) = std::env::var_os("STREAMSMASH_WORKSPACE_PREVIEWS") {
                    let mut ppm = format!("P6\n{width} {height}\n255\n").into_bytes();
                    for p in &pixels {
                        ppm.extend([p.r, p.g, p.b]);
                    }
                    std::fs::write(
                        std::path::PathBuf::from(dir).join(format!("{width}-basic-{tab}.ppm")),
                        ppm,
                    )
                    .unwrap();
                }
            }
        }
        ui.set_microphone_enabled(true);
        ui.set_microphone_gain(75);
        ui.set_audio_gain(65);
        ui.set_audio_limiter(true);
        ui.set_microphones(strings([
            "Windowsの既定マイク".into(),
            "マイク（テスト用）".into(),
        ]));
        ui.set_microphone_index(1);
        for (width, height) in [(1280, 860), (1040, 720)] {
            surface.set_size(slint::PhysicalSize::new(width, height));
            ui.invoke_show_settings(0);
            for tab in [0, 2] {
                ui.set_basic_tab(tab);
                std::thread::sleep(Duration::from_millis(180));
                slint::platform::update_timers_and_animations();
                ui.window().request_redraw();
                let mut pixels = vec![slint::Rgb8Pixel::default(); (width * height) as usize];
                surface.draw_if_needed(|r| {
                    r.render(&mut pixels, width as usize);
                });
                if let Some(dir) = std::env::var_os("STREAMSMASH_WORKSPACE_PREVIEWS") {
                    let mut ppm = format!("P6\n{width} {height}\n255\n").into_bytes();
                    for p in &pixels {
                        ppm.extend([p.r, p.g, p.b]);
                    }
                    std::fs::write(
                        std::path::PathBuf::from(dir).join(format!("{width}-microphone-{tab}.ppm")),
                        ppm,
                    )
                    .unwrap();
                }
            }
        }
        ui.set_microphone_enabled(false);
        ui.set_expanded(true);
        ui.invoke_show_settings(3);
        assert!(!ui.get_expanded());
        ui.invoke_show_workspace();
        assert!(!ui.get_settings_open());
        // Full replay stop must release the heavy path only while idle, and
        // reapply capture via the existing action (spied here, no devices).
        let restarts = Rc::new(std::cell::Cell::new(0));
        let seen_restarts = restarts.clone();
        ui.on_start_preview(move || seen_restarts.set(seen_restarts.get() + 1));
        setup_auto_replay(
            &ui,
            Rc::new(Controller::new()),
            Rc::new(RefCell::new(None)),
            Rc::new(RefCell::new(None)),
        );
        ui.set_busy(false);
        ui.set_buffering(true);
        ui.set_replay_enabled(true);
        ui.set_streaming(true);
        ui.invoke_auto_action(17);
        assert!(ui.get_replay_enabled());
        assert_eq!(restarts.get(), 0);
        ui.set_streaming(false);
        ui.set_recording(true);
        ui.invoke_auto_action(17);
        assert!(ui.get_replay_enabled());
        assert_eq!(restarts.get(), 0);
        ui.set_recording(false);
        ui.invoke_auto_action(6);
        assert!(ui.get_replay_enabled());
        assert!(ui.get_auto_status().contains("蓄積"));
        ui.invoke_auto_action(17);
        assert!(!ui.get_replay_enabled());
        assert!(!ui.get_auto_monitoring());
        assert_eq!(restarts.get(), 1);
        // A late GPU probe result must never change a live session or a new size.
        ui.set_encoder_index(0);
        ui.set_quality_index(2);
        let requested = probe_request(&ui);
        let selection = || {
            Ok(streamsmash_media::encoder_probe::Selection {
                encoder: Some(Encoder::Amd),
                tested_outputs: 3,
            })
        };
        ui.set_streaming(true);
        apply_encoder_probe_reply(
            &ui,
            streamsmash_media::encoder_probe::Reply {
                request: requested,
                result: selection(),
            },
        );
        assert_eq!(ui.get_encoder_index(), 0);
        ui.set_streaming(false);
        ui.set_quality_index(0);
        apply_encoder_probe_reply(
            &ui,
            streamsmash_media::encoder_probe::Reply {
                request: requested,
                result: selection(),
            },
        );
        assert_eq!(ui.get_encoder_index(), 0);
        apply_encoder_probe_reply(
            &ui,
            streamsmash_media::encoder_probe::Reply {
                request: probe_request(&ui),
                result: selection(),
            },
        );
        assert_eq!(ui.get_encoder_index(), 1);
        apply_encoder_probe_reply(
            &ui,
            streamsmash_media::encoder_probe::Reply {
                request: probe_request(&ui),
                result: Err("fixture failure".into()),
            },
        );
        assert_eq!(ui.get_encoder_index(), 1);
        apply_encoder_probe_reply(
            &ui,
            streamsmash_media::encoder_probe::Reply {
                request: probe_request(&ui),
                result: Ok(streamsmash_media::encoder_probe::Selection {
                    encoder: None,
                    tested_outputs: 0,
                }),
            },
        );
        assert_eq!(ui.get_encoder_index(), 0);
        assert_eq!(scratch.parent(), Some(std::env::temp_dir().as_path()));
        std::fs::remove_dir_all(scratch).unwrap();
        ui.hide().unwrap();
    }
}
