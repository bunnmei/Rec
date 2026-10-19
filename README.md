# レコード録音（RecordToCD）

USB端子つきのレコードプレーヤー（Sony PS-LX310BT / PS-LX300USB など）の音を録音して、
**曲ごとに自動で分けて、音楽CDを作る**ためのアプリです。WAV・MP3でパソコンに保存することもできます。

- 針を落とすと**自動で録音が始まり**、レコードが終わると**自動で止まります**
- A面が終わると「裏返してください」と案内し、B面も続けて録音できます
- 曲と曲のあいだの無音を見つけて**自動で曲を分けます**（あとから手で直せます）
- CDプレーヤーや車で聴ける**音楽CD**を作れます（最大80分）
- 文字・ボタンは大きく、画面は「操作」と「設定」の2つだけ。明るい画面・暗い画面を選べます

## 使い方

画面は1ページで、下へスクロールすると ①〜③ の区画が並んでいます。**①〜③はそれぞれ別々に使えます。**

1. **録音する** — 「A面の録音をはじめる」を押して、レコードに針を落とします。
   録音中は波の形が表示され、レコードが終わると自動で止まります。B面も同じように録音します。
   曲に分けず、録音したままの音（A面・B面それぞれ1つのファイル）を保存することもできます。
2. **曲を分けて保存する** — 録音した音、またはパソコンにある音声ファイル（WAV・MP3・FLAC・M4A・OGG）を
   自動で曲ごとに分けます。波の絵を押すとその場所から聴けます。赤い線（▼）が曲の区切りで、
   押すと「少し前へ／少し後ろへ／消す」ができます。曲名を付けて、曲ごとに WAV・MP3 で保存できます。
3. **音楽CDを作る** — ②で分けた曲、または**パソコンにある曲を選んで**（順番も変えられます）音楽CDを作ります。

作業の途中でアプリを閉じても、次に開いたときに続きから始められます。

## ダウンロードとインストール

[Releases](../../releases) から、お使いのパソコンに合ったファイルをダウンロードしてください。

| パソコン | ファイル |
| --- | --- |
| Mac（M1 以降） | `RecordToCD_x.y.z_aarch64.dmg` |
| Windows（64bit） | `RecordToCD_x.y.z_x64-setup.exe`（または `.msi`） |
| Linux（Ubuntu・Debian など） | `RecordToCD_x.y.z_amd64.deb` |
| Linux（Fedora など） | `RecordToCD-x.y.z-1.x86_64.rpm` |

### Mac で初めて開くとき

このアプリは Apple の有料の署名をしていないため、初めて開くときに警告が出ます。

1. dmg を開き、「RecordToCD」を「アプリケーション」フォルダへドラッグします。
2. アプリを開こうとして「開けません」と出たら、**システム設定 → プライバシーとセキュリティ** を開き、
   下の方にある **「このまま開く」** を押します。
3. 「壊れているため開けません」と出る場合は、ターミナルで次を実行してから開いてください。
   ```sh
   xattr -cr /Applications/RecordToCD.app
   ```
4. 「マイクへのアクセス」を聞かれたら **「許可」** を押します（レコードプレーヤーの音を受け取るために必要です）。

CDの書き込みには macOS 標準の機能（drutil）を使うので、ほかに入れるものはありません。

### Windows で初めて開くとき

「WindowsによってPCが保護されました」と出たら、**「詳細情報」→「実行」** を押してください。
CDの書き込みには Windows 標準の機能（IMAPI2）を使います。

### Linux

CDの書き込みに `cdrskin` を使います。下のようにインストールすると一緒に入ります。

```sh
# Ubuntu・Debian など
sudo apt install ./RecordToCD_*_amd64.deb
# Fedora など
sudo dnf install ./RecordToCD-*.x86_64.rpm
```

## 自動で止まる・曲を分ける仕組み

- 0.05秒ごとに音の大きさを測り、**1秒のうち音がある時間の割合**で「鳴っている」かを判断します。
  レコードの最後の溝の「プチッ、プチッ」という周期的な音では止まらなくならないようにしています。
- 音が途切れてから一定時間（設定。ふつうは15秒）たつか、片面の最大時間（LP 30分・EP 10分・自由設定）に
  達すると録音を止めます。
- 曲分けは、針音の大きさ（録音全体で一番静かな部分）を基準に、それより少し大きい音までを「無音」とみなし、
  1.5秒以上の無音を曲間とします（設定の「曲の分け方」で弱い・ふつう・強いを選べます）。
  30秒より短い曲はできないようにしています。
- CD用には 44.1kHz・16bit に変換し、曲の頭と終わりに短いフェードをかけてプチッという音を防ぎます。

## 開発

必要なもの: Node.js 22 以上、Rust（stable）

Linux では次のライブラリも必要です。

```sh
# Fedora
sudo dnf install webkit2gtk4.1-devel alsa-lib-devel cdrskin
# Ubuntu・Debian
sudo apt install libwebkit2gtk-4.1-dev libasound2-dev librsvg2-dev cdrskin
```

```sh
npm install
npm run tauri dev       # 開発用に起動
npm run check           # 画面側の型チェック
cd src-tauri && cargo test   # 録音・曲分け・書き出しのテスト

# 実際の機器で確かめる（レコードプレーヤーをつないでから）
cd src-tauri && cargo test -- --ignored --nocapture
```

### 配布（リリース）のしかた

1. `package.json`・`src-tauri/tauri.conf.json`・`src-tauri/Cargo.toml` の `version` を上げます。
2. コミットして、タグを付けて push します。
   ```sh
   git tag v0.1.0
   git push origin v0.1.0
   ```
3. GitHub Actions が Mac（Apple Silicon）・Windows（64bit）・Linux（deb / rpm）のファイルを作り、
   Releases に**下書き**として置きます。中身を確かめてから「Publish」してください。

Apple の開発者証明書をお持ちの場合は、リポジトリの Secrets に
`APPLE_CERTIFICATE` などを登録すると、署名・公証された dmg になります（`.github/workflows/release.yml` 参照）。

### ファイルの構成

```
src/                    画面（Svelte）
  lib/MainScreen.svelte     操作画面（①〜③を縦に並べる）
  lib/RecordSection.svelte  ① 録音（録音中の波形つき）
  lib/SplitSection.svelte   ② 曲を分けて保存する（SplitEditor.svelte が波形と区切りの編集）
  lib/CdSection.svelte      ③ 音楽CDを作る
  lib/SettingsScreen.svelte 設定画面
src-tauri/src/
  audio/recorder.rs     録音と自動停止
  audio/analysis.rs     波形・無音の検出・自動曲分け
  audio/export.rs       曲の切り出し、WAV・MP3 書き出し
  audio/decode.rs       パソコンにある音声ファイル（MP3・FLAC など）の読み込み
  audio/resample.rs     44.1kHz への変換
  burn/                 CD書き込み（Windows: IMAPI2 / macOS: drutil / Linux: cdrskin）
  project.rs            作業中のデータの保存と復元
```
