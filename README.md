# Stream Smash 0.2.0

OBSを経由せず、ゲーム映像の録画と配信を行うオープンソースのWindowsアプリです。再構築版のエンジンはRust、Windowsの操作画面はWinForms、映像・音声の処理はFFmpegを使用します。

## Windowsで起動

DirectShow・libx264対応の [FFmpeg](https://ffmpeg.org/download.html#build-windows) が必要です。FFmpegはPATHに入れるか、`STREAM_SMASH_FFMPEG` に絶対パスを指定してください。

配布ZIPを展開し、`StreamSmash.exe` を開きます。同じフォルダの `stream-smash.exe` が録画エンジンです。ソースからビルドする場合は [Rust](https://rustup.rs/) を用意し、リポジトリのルートで `build-windows.cmd` を実行して `target\release\StreamSmash.exe` を開きます。

- **テスト映像・音声** で録画を試せます。**キャプチャー機器** を選ぶと接続機器を検索し、映像機器・音声機器の候補を表示します。必要に応じて機器名を直接入力し、機器が対応する解像度とfpsを選んでください。
- **配信 URL** を入力すると、MKV録画と配信が同時に始まります。空欄の場合は録画だけを行います。
- 録画中は経過時間、出力フレーム、エンコーダーでのドロップ・重複、配信リトライ回数を表示します。受信状況は配信先の画面で確認してください。

## 見せ場を切り出す

録画前に **見せ場の長さ** で10・30・60秒を選びます。録画中に **見せ場を保存** を押すか、Stream Smashの操作画面にフォーカスがある状態で **F8** を押すと、その場面までの映像と音声が録画先に別のMKVとして保存されます。録り始めや停止直前では、録画済みの範囲が保存されます。保存には通常数秒かかります。

**音の盛り上がりを自動保存** をオンにすると、入力音声が普段より急に大きくなった場面を検出し、自動でクリップを残します。連続保存を抑えるため、自動検出には12秒の間隔を設けています。ゲーム画面の撃墜やGAME SETを認識する機能は未実装です。

## コマンドライン

```powershell
cargo run --locked -- record --test-pattern --size 1280x720 --fps 30 --seconds 5
cargo run --locked -- devices --names
```

録画を途中で終了する場合は、`q` を入力して Enter を押します。録画開始時に `--highlights 30` を付けると `h` と Enter で見せ場を保存でき、`--audio-peaks` を追加すると音量変化でも保存できます。CLIから同時配信する場合は環境変数 `STREAM_SMASH_RTMP_URL` を設定し、`record` に `--stream` を付けます。配信URLは設定ファイルに保存されません。

## 検証と開発状況

録画用エンコーダーとRTMP送信を別プロセスで動かし、配信が切れた場合は送信側だけを再起動します。WindowsのローカルRTMPテストでは、受信接続を切断・復旧させた間も、15秒の録画に音声と450フレームがすべて残りました。

Python 3、FFmpeg、FFprobeを用意し、`cargo build --locked` の後に `python tests/highlights.py`、`python tests/audio_highlights.py`、`python tests/rtmp_reconnect.py` を実行すると、クリップ保存とローカル配信の切断・復旧を検証できます。

AVerMedia実機での映像・音声入力、YouTube本番配信、macOS・Linuxでの動作は未検証です。旧版のライブプレビュー、GPUエンコード、バースト自動クリップは再構築版に未移植です。[旧バージョンから復元できたソース](https://github.com/daichingale/stream-smash/tree/recovery/rust-snapshots/recovered)
