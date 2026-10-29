# GitHub公開手順

MioNote 1.0.0のソースを、独立したリポジトリとして公開するための手順です。

## 公開用フォルダーの作成

プロジェクトのルートでPowerShell 7から実行します。

```powershell
.\package-source.ps1 -GitHub
```

`dist/MioNote-1.0.0-github/` と `dist/MioNote-1.0.0-github.zip`、ZIPのSHA-256を作成します。
フォルダーが既にある場合は停止します。そこで加えた編集を保存してから、フォルダーを別の場所へ移すか名前を変えて再実行してください。

## リポジトリへ登録する内容

ZIPを展開し、`README.md`、`Cargo.toml`、`src/`、`.cargo/` などがあるフォルダーの**中身**をリポジトリのルートへ登録します。

- `src/` とテスト用の合成ノート
- Rustの固定バージョン、`Cargo.lock`、ビルド・配布スクリプト
- 新旧のアイコン、生成プロンプト、README用の実画面
- MITライセンス、依存ライブラリのライセンス表記
- 保存形式・入力検証・性能・検証結果の文書
- プライバシーポリシーとStore提出用の設定・説明文

実行ファイルや配布ZIPは、ビルド後にGitHub Releasesへ添付する形で配布できます。
公開用ソースには、SDK・Rustのダウンロードキャッシュ、`target/`、`dist/`、個人のノート、設定、診断ログ、署名鍵は含めません。
唯一の `.inknote` は `tests/fixtures/v2-mixed-paper.inknote` で、互換性テスト用の合成データです。
詳細なWACK原本と実機試験ログは検証端末の作業用ファイルであり、公開用のJSON結果を `docs/` に含めています。

## Gitを使う場合

GitHubで空のリポジトリを作り、展開したフォルダーで次のコマンドを実行します。
`YOUR_ACCOUNT` とリポジトリ名は自分のものへ置き換えてください。Gitのユーザー名・メールアドレスは、事前に自分の環境で設定します。

```powershell
git init -b main
git add .
git diff --cached --stat
git commit -m "Initial release of MioNote 1.0.0"
git remote add origin https://github.com/YOUR_ACCOUNT/MioNote.git
git push -u origin main
```

GitHubのファイルアップロードを使う場合も、ZIPをそのまま1ファイルとして登録するのではなく、展開した中身を登録します。

## 他の開発環境でのビルド

Windows 11、PowerShell 7、Rustup、対象CPU向けのMSVC C++ツールとWindows SDKを用意します。
対応するDeveloper PowerShellで `build.ps1` を実行します。ビルド対象と同じCPUのWindows端末から始めると設定が少なく済みます。

```powershell
# ARM64 Windows
.\build.ps1 -Architecture arm64

# x64 Windows
.\build.ps1 -Architecture x64
```

使用するRustは `rust-toolchain.toml` に固定されています。初回はRustupとCargoがツールチェーン・依存crateを取得します。
`build.ps1` には開発元の相対パス `../tools` を検出する補助がありますが、このフォルダー自体は不要です。
通常のRustup・MSVC・Windows SDKがPATH／LIBに設定されていればビルドできます。
ビルド時のリソースコンパイラーが見つからない場合は、環境変数 `RC` にSDKの `rc.exe` の絶対パスを指定します。

ARM64ホスト上でのx64クロスビルドでは、Rustのホスト・ビルドスクリプト用ライブラリと、対象CPU向けMSVC／SDKライブラリの組み合わせにも注意してください。
動作確認と開発上の規約は [CONTRIBUTING.md](../CONTRIBUTING.md) にあります。

## Storeの本人性と改行コード

`distribution/store/identity.json` のPackage NameとPublisherは、MioNoteの公開識別情報です。秘密鍵や署名用の認証情報ではありません。
自分のアプリとしてStoreへ提出する場合は、自分が予約したアプリの値へ差し替えます。
`distribution/store/runFullTrust.txt` は、現在のWin32／mediumIL構成に対する承認申請用の説明文です。

`.gitattributes` はGitによる改行コードの自動変換を止め、監査対象の `Cargo.lock` などのバイト列を維持します。
`SOURCE-MANIFEST.sha256` は通常の配布用ソースZIPだけに生成し、GitHub用フォルダーには追加しません。

## ライセンス

ソースコードとアイコンは [MIT License](../LICENSE) です。依存ライブラリはそれぞれのライセンスに従います。
公開用フォルダーにライセンス本文と依存関係の表記を含めています。
