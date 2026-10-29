# MioNote 1.0.0 — 公開前の検証（2026-10-27）

アプリ本体・EXEのFileVersion／ProductVersionは **1.0.0**。Store提出用MSIXのバージョンは **1.0.0.0**。
StoreのName・Publisher・PublisherDisplayNameは利用者から提供された正式値を継続し、初回提出用のパッケージバージョンは増やしていない。

## アイコンとコピーライト

- 「水筆（みお）」のしずく、筆で書く少女、藍色と生成りの影絵を維持。髪飾り・着物の細線・波を省き、人物を大きくした。
- 新しい[原画](../assets/branding/mio-note-v2.png)は組み込み画像生成ツールで作成。[生成プロンプト](../assets/branding/mio-note-v2.prompt.txt)もソースZIPに含む。
- ICOの16／20／24／32／40／48／64／128／256pxと、Store画像の50／44／150pxを更新。小さい表示と明暗両方の背景で見え方を確認した。PNGのサイズ・透過とICO各フレームの整合性も確認した。
- 元の[絵](../assets/branding/mio-note-v1.png)を変更せず保持。コピーライト表示用ICOも旧0.12.1の配布用ICOと完全一致する。
- ツールバーの「©」またはF1で「バージョン情報・コピーライト」を開き、元の絵を大きく表示する。旧画像はこの画面を開いたときだけLoadImageWで読み込む。終了時はHICONを解放する。
- 新しい外部ライブラリー、WebView、常駐処理、タイマー、通信処理は追加していない。著作権表記は既存のMITライセンスと一致させ、発行元を併記した。

## ビルドと画面操作

Windows 11 ARM64（10.0.26300.0）で確認。ARM64はネイティブ、x64はWindows on Armのエミュレーションで実行した。
画面操作にはcomputer-useスキルを使用し、ユーザー設定と分離したテスト用ポータブルフォルダーを使った。

| 確認 | ARM64 | x64 |
| --- | --- | --- |
| Releaseビルドの自動テスト | 36件PASS | 36件PASS |
| Clippy（全ターゲット、警告をエラー扱い） | 通常版・Store版PASS | 通常版・Store版PASS |
| 起動・新しいウィンドウアイコン・©ボタンの配置 | 確認済み | 確認済み |
| ©ボタン／F1でコピーライトを表示 | 確認済み | 確認済み |
| 元の絵・1.0.0・著作権・発行元の表示 | 確認済み | 確認済み |
| Esc／Enterで閉じる | 確認済み | 確認済み |
| 閉じた直後のCtrl+2（100%へ復帰） | 確認済み | 確認済み |

rustfmtもPASS。保存先のハードリンク攻撃対策を含む全テストは、ハードリンク作成が許される通常のWindows権限環境で実行した。
全4ビルド（通常版・Store版×2 CPU）で、新旧の各9画像フレームがEXEに正しく埋め込まれていることを確認した。
静的CRT、ASLR・DEP・CFG、System32限定の依存DLL読み込みも検証済み。
依存関係は45件をOSVで照合し指摘なし。[監査結果](dependency-audit-20261027.json)は現在のCargo.lockのSHA-256と一致する。

## WACK

Microsoft署名のあるWACK 10.0.28000.2957（x86）をこのARM64端末で実行した。
3提出物すべて **24項目PASS**、PARTIAL_RUN=FALSE、メッセージ0件。検査前後のSHA-256も一致する。

| 対象 | 結果 | 生XML | HTML |
| --- | --- | --- | --- |
| ARM64＋x64 bundle | 24件PASS | [XML](../target/wack-1.0.0-20261027/bundle-report.xml) | [HTML](../target/wack-1.0.0-20261027/bundle-report.html) |
| ARM64 MSIX | 24件PASS | [XML](../target/wack-1.0.0-20261027/arm64-report.xml) | [HTML](../target/wack-1.0.0-20261027/arm64-report.html) |
| x64 MSIX | 24件PASS | [XML](../target/wack-1.0.0-20261027/x64-report.xml) | [HTML](../target/wack-1.0.0-20261027/x64-report.html) |

Centennialアプリ向けのファイル検査であり、WACK自身はパッケージをインストールしていない。
bundleで選択されたx64ペイロードに加えて、ARM64の個別MSIXも独立して検査した。
詳細と提出物SHA-256は[機械可読レポート](wack-results-1.0.0-20261027.json)に記録した。
ローカルの生XML／HTMLは `target/wack-1.0.0-20261027/` に保持し、ソースZIPには機械可読レポートを収録する。

## 配布物と確認範囲

- [Store提出用bundle](../dist/MioNote-1.0.0-store.msixbundle)（ARM64＋x64、未署名）
- [ポータブル版ARM64](../dist/MioNote-1.0.0-portable-arm64.zip)／[ポータブル版x64](../dist/MioNote-1.0.0-portable-x64.zip)
- [ソースZIP](../dist/MioNote-1.0.0-source.zip)

Storeへのアップロード・公開や証明書ストアの変更は行っていない。既存のユーザー設定は検証前のハッシュと一致する。
1.0.0の画面確認はポータブル版で実施した。署名済みパッケージのインストール・設定復元・更新・アンインストール等は
[0.12.1の実動作確認](store-runtime-validation-20261027.md)を参照。今回その全工程を再実施したものではない。
Intel／AMDの実機と、実ペンによる筆圧・遅延は未検証。

配布ZIPの許可リスト、MSIXの同一性・マニフェスト・CRC・ブロックハッシュ、ソースZIPのハッシュは
`scripts/verify-packages.py` で照合する。個人ノート・設定・診断ログ・証明書・一時検証スクリプトは配布物に含めない。
