# Stream Smash

OBSを経由せず、ゲーム映像の録画と配信を行うオープンソースのWindowsアプリです。再構築版のエンジンはRust、Windowsの操作画面はWinForms、映像・音声の処理はFFmpegを使用します。

## Windowsで起動

[Rust](https://rustup.rs/) と、DirectShow・libx264対応の [FFmpeg](https://ffmpeg.org/download.html#build-windows) が必要です。FFmpegはPATHに入れるか、`STREAM_SMASH_FFMPEG` に絶対パスを指定してください。

リポジトリのルートで `build-windows.cmd` を実行し、`target\release\StreamSmash.exe` を開きます。同じフォルダの `stream-smash.exe` が録画エンジンです。

- **Test signal** で合成映像とテスト音を録画できます。**Windows capture device** は **List devices** で表示されたデバイス名を使います。接続機器が対応する解像度とfpsを指定してください。
- **RTMP(S) URL** を入力すると、MKV録画と配信が同時に始まります。空欄の場合は録画だけを行います。
- 録画中は経過時間、出力フレーム、エンコーダーでのドロップ・重複、配信リトライ回数を表示します。受信状況は配信先の画面で確認してください。

## コマンドライン

```powershell
cargo run --locked -- record --test-pattern --size 1280x720 --fps 30 --seconds 5
cargo run --locked -- devices
```

録画を途中で終了する場合は、`q` を入力して Enter を押します。CLIから同時配信する場合は環境変数 `STREAM_SMASH_RTMP_URL` を設定し、`record` に `--stream` を付けます。配信URLは設定ファイルに保存されません。

## 検証と開発状況

録画用エンコーダーとRTMP送信を別プロセスで動かし、配信が切れた場合は送信側だけを再起動します。WindowsのローカルRTMPテストでは、受信接続を切断・復旧させた間も、15秒の録画に音声と450フレームがすべて残りました。

同じ切断テストを再現するには、Python 3、FFmpeg、FFprobeを用意し、`cargo build --locked` の後に `python tests/rtmp_reconnect.py` を実行します。テストはローカル通信だけを使います。

AVerMedia実機での映像・音声入力、YouTube本番配信、macOS・Linuxでの動作は未検証です。旧版のライブプレビュー、GPUエンコード、バースト自動クリップは再構築版に未移植です。[旧バージョンから復元できたソース](https://github.com/daichingale/stream-smash/tree/recovery/rust-snapshots/recovered)
