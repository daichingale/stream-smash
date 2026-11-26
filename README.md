# Stream Smash 0.3.0

OBSを経由せず、ゲーム映像の録画と配信を行うオープンソースのWindowsアプリです。再構築版のエンジンはRust、Windowsの操作画面はWinForms、映像・音声の処理はFFmpegを使用します。

## Windowsで起動

DirectShow・libx264対応の [FFmpeg](https://ffmpeg.org/download.html#build-windows) が必要です。FFmpegはPATHに入れるか、`STREAM_SMASH_FFMPEG` に絶対パスを指定してください。

[GitHubのソース一式をZIPで取得](https://github.com/daichingale/stream-smash/archive/refs/heads/main.zip) またはクローンし、ルートにある `StreamSmash.exe` を開きます。Windows x64用のビルド済みアプリと、同じフォルダに必要な録画エンジン `stream-smash.exe` が入っています。録画エンジンを直接使う場合は `stream-smash.exe --help` でコマンドを確認できます。

コードを変更してビルドする場合は [Rust](https://rustup.rs/) を用意し、ルートで `build-windows.cmd` を実行します。ビルドし直したアプリとエンジンは `target\release` に出力されます。

- **テスト映像・音声** で録画を試せます。**キャプチャー機器** を選ぶと接続機器を検索し、映像機器・音声機器の候補を表示します。必要に応じて機器名を直接入力し、機器が対応する解像度とfpsを選んでください。
- **配信 URL** を入力すると、MKV録画と配信が同時に始まります。空欄の場合は録画だけを行います。
- 録画中は経過時間、エンコード済みフレームと処理fps、間引き・重複、配信リトライ回数を表示します。受信状況は配信先の画面で確認してください。

## 配信画質と負荷を調整する

| 画質プリセット | 出力 | 映像ビットレート目標 |
| --- | --- | --- |
| 軽量 | 720p60 | 6 Mbps |
| 標準 | 1080p60 | 12 Mbps |
| 高画質 | 1080p60 | 17 Mbps |
| 高解像度 | 1440p60 | 34 Mbps |

プリセット適用後も出力解像度、fps、映像ビットレートを変更できます。**エンコーダー** の初期設定は **自動（GPU優先）** です。同じ出力解像度とfpsで短いテストを行い、AMD AMF、NVIDIA NVENC、Intel Quick Syncの順に使えるものを選びます。利用できるGPUがなければCPUで処理します。GPUを直接指定した場合は、そのGPUの初期化に失敗するとログに表示されます。

**キャプチャー解像度** を使うと、たとえば機器から1080pで入力して720pで配信できます。**機器の既定** を選ぶと、入力の解像度とfpsは機器に任せます。入力と出力のサイズが揃うテスト映像とDirectShowの指定入力では、スケーリング処理を省きます。

## 見せ場を切り出す

録画前に **見せ場の長さ** で10・30・60秒を選びます。録画中に **見せ場を保存** を押すか、Stream Smashの操作画面にフォーカスがある状態で **F8** を押すと、その場面までの映像と音声が録画先に別のMKVとして保存されます。録り始めや停止直前では、録画済みの範囲が保存されます。保存には通常数秒かかります。

**音の盛り上がりを自動保存** をオンにすると、入力音声が普段より急に大きくなった場面を検出し、自動でクリップを残します。連続保存を抑えるため、自動検出には12秒の間隔を設けています。ゲーム画面の撃墜やGAME SETを認識する機能は未実装です。

## コマンドライン

```powershell
cargo run --locked -- record --test-pattern --size 1280x720 --fps 30 --seconds 5
cargo run --locked -- record --test-pattern --encoder auto --quality detail --bitrate 17000 --seconds 5
cargo run --locked -- devices --names
```

録画を途中で終了する場合は、`q` を入力して Enter を押します。録画開始時に `--highlights 30` を付けると `h` と Enter で見せ場を保存でき、`--audio-peaks` を追加すると音量変化でも保存できます。CLIから同時配信する場合は環境変数 `STREAM_SMASH_RTMP_URL` を設定し、`record` に `--stream` を付けます。配信URLは設定ファイルに保存されません。

## 検証と開発状況

録画用エンコーダーとRTMP送信を別プロセスで動かし、配信が切れた場合は送信側だけを再起動します。WindowsのローカルRTMPテストでは、受信接続を切断・復旧させた間も、15秒の録画に音声と450フレームがすべて残りました。

AMD AMFの1080p60・17 Mbps設定でも同じ切断テストを行い、15秒の録画に音声と900フレームが残ることを確認しました。別途、同じ設定の合成映像360フレームを3回ずつ非リアルタイムでエンコードしたところ、中央値のFFmpeg CPU時間はCPU（x264）24.97秒、AMD AMF 3.47秒でした。この計測はエンコード処理を対象とし、実配信全体のCPU使用率は未計測です。

Python 3、FFmpeg、FFprobeを用意し、`cargo build --locked` の後に `python tests/highlights.py`、`python tests/audio_highlights.py`、`python tests/rtmp_reconnect.py` を実行すると、クリップ保存とローカル配信の切断・復旧を検証できます。

WindowsでAMD AMFの動作を検証しています。NVIDIA NVENC、Intel Quick Sync、AVerMedia実機の入力、YouTube本番配信、macOS・Linuxでの動作は未検証です。旧版のライブプレビューと撃墜・試合終了の画像認識は再構築版に未移植です。[旧バージョンから復元できたソース](https://github.com/daichingale/stream-smash/tree/recovery/rust-snapshots/recovered)
