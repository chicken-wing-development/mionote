# Store版の実動作確認 — 2026-10-27

MioNote **0.12.1**について、この端末で実行できる署名済みパッケージの動作確認を完了した。検査中に筆記・ファイルダイアログ後のキーボードフォーカス復帰を補強し、最終ビルドで保存直後のショートカットまで確認した。

## 対象と方法

- Windows 11 Home、10.0.26300.0、ARM64。ARM64版はネイティブ、x64版はWindows on Armのエミュレーションで実行。
- 提出候補はパッケージバージョン1.0.0.0。更新試験だけに1.0.2.0を使用した。`distribution/store/identity.json` の提出用バージョンは1.0.0.0のまま。
- 未署名の提出候補を別フォルダーへコピーし、有効期間1日の自己署名コード署名証明書で署名した。Microsoft Storeが署名した配布物ではなく、公開前のローカル検証である。
- 証明書は非CA・コード署名用・秘密鍵のエクスポート不可。公開証明書だけをLocalMachineのTrustedPeopleへ一時登録した。Rootへの登録や開発者モードの有効化は行っていない。
- 署名済みパッケージの各110ペイロードを提出候補と照合。更新試験版はマニフェストのバージョンだけを変更し、実行ファイルなどの内容が同じであることを確認した。署名時にSDKが追加する署名とCodeIntegrityカタログは区別している。
- 画面操作はcomputer-useスキルで行い、筆記はマウスのドラッグを用いた。実際のペンの筆圧応答・遅延は測定していない。

## 結果

| 確認項目 | 結果 | 確認内容 |
| --- | --- | --- |
| ARM64のインストール | PASS | 署名済みbundleからARM64版が選択され、通常権限で起動 |
| x64の新規インストール | PASS | 提出候補と同じ1.0.0.0の単体MSIXを導入し、最終EXEのSHA-256を照合 |
| 設定の保存・復元 | PASS | ARM64は太・筆圧オン・広い罫線、x64は中の線幅を設定。LocalStateへの保存と再起動後の選択状態を確認 |
| ノートの保存・読み込み | PASS | 日本語フォルダー・日本語ファイル名、別名保存、A4縦とJIS B5横の2ページ、追記後の再読み込みを確認 |
| 筆記直後のショートカット | PASS | ツールバーに触れずCtrl+S／Ctrl+Shift+Sで保存。保存ダイアログを閉じた直後のCtrl+Eも確認 |
| PDF・PNG・SVG | PASS | 最終ARM64・x64の両方で出力。PDF全ページをレンダリングして筆跡を確認し、用紙寸法も照合 |
| 全ページの画像ZIP | PASS | 最終ARM64でPNG／SVGのページ順、CRC、用紙別寸法と、単体出力との一致を確認 |
| パッケージ更新 | PASS | ARM64はbundle、x64は単体MSIXで1.0.0.0から1.0.2.0へ更新。設定ファイルのSHA-256が一致し、更新後も起動・復元 |
| アンインストール | PASS | 両CPU版の登録と設定ファイルを削除。利用者が指定した場所のノート・出力ファイルは削除前後のSHA-256が一致 |
| 通常時のログ抑止 | PASS | 通常起動と設定変更でLocalStateに診断ログが作られないことを確認 |
| ポータブル版との設定分離 | PASS | 初期0.12.0の試験で別々の設定を読み、ポータブルの変更後もStore設定のハッシュが一致。0.12.1で保存先の実装は変更なし |
| 不正な入力 | PASS | 初期0.12.0の試験で不正なZIP形式を拒否し、開いていたノートと筆跡を保持。ローダーは変更せず、最終コードのネイティブテストも再実行 |

最終ARM64のPDFは210×297 mmと257×182 mm、現在ページのPNGは3035×2150ピクセル・約300dpi。x64のPDFは210×297 mm、PNGは2480×3508ピクセル・約300dpi。SVGのmm寸法も一致した。

ISFは再エンコードでバイト列が変わるため、保存・読み込みの品質をバイト一致だけで判定していない。初期の往復保存ではページ情報と筆跡の構造が維持され、PDF描画座標の最大差は約0.013 mmだった。最終ビルドでも再起動後にページ数・向き・筆跡を画面と出力ファイルで確認した。

bundleから導入した検証版を単体MSIXで更新する試みはWindowsの依存関係検証で0x80073CF3となった。同形式のbundleで更新すると成功したため、bundleの手動更新もbundleを使う。Store提出用bundleの作成方法は変更していない。

## 修正と再検証

`src/ui.rs` に、対象ウィンドウがフォアグラウンドで、キーボードフォーカスがない場合だけ復帰する共通処理を追加した。筆記・I/O完了時と、ファイルダイアログから戻ったときに呼ぶ。他のアプリや表示中のダイアログのフォーカスを変更しない。常時監視、タイマー、グローバルキーフックは追加していない。

ファイルダイアログのエラー番号は、フォーカス処理を呼ぶ前に取得する。既存のキャンセル判定・入力検証は維持した。

最終コードでARM64・x64の各36テスト、両CPUの通常／Storeの全4構成のClippy、rustfmtが成功した。MakeAppxのスキーマ・意味検証、静的CRT、ASLR・DEP・CFG、System32限定のDLL依存読み込みも確認した。依存45件のOSV照合は検出0件。

最終提出候補のWACKもbundle・ARM64・x64の各24項目がPASS、部分実行フラグはFALSE、メッセージ欄は0件。[WACKの結果と原本](wack-validation-20261027.md)を参照。

## 検証後の状態

検証用アプリの登録、LocalStateの設定、一時証明書のTrustedPeopleへの登録、CurrentUserの証明書と秘密鍵を削除した。Rootにも該当証明書がないことを別のプロセスで確認した。既存の通常版・ポータブル版の設定と、旧0.12.0配布物もハッシュが一致している。

公開用ソースには個人の端末名・ローカルパスを含む詳細ログや検証用署名鍵を同梱しない。構造化した結果は [store-runtime-results-20261027.json](store-runtime-results-20261027.json) に保存した。原本と合成ノートは、この検証端末の `target/store-runtime-20261027` に保管している。

Storeへのアップロード・公開は行っていない。Microsoft Store経由のダウンロード・更新、実物のIntel／AMD端末、実ペンの筆圧・遅延は今回の確認範囲に含まない。プライバシーポリシーとサポートURLの公開、Partner Centerの提出情報は [公開手順](store-release.md) の残りの工程を参照。

## 最終配布物

- [Microsoft Store提出用bundle（未署名）](../dist/MioNote-0.12.1-store.msixbundle)
- [ARM64ポータブルZIP](../dist/MioNote-0.12.1-portable-arm64.zip)
- [x64ポータブルZIP](../dist/MioNote-0.12.1-portable-x64.zip)
- [ソースZIP](../dist/MioNote-0.12.1-source.zip)

検証用の自己署名パッケージは公開用配布物に含めない。

## 一次資料

- [Microsoft: パッケージ署名用の証明書と一時信頼の後片付け](https://learn.microsoft.com/en-us/windows/msix/package/create-certificate-package-signing)
- [Microsoft: SignToolによるMSIX／bundle署名](https://learn.microsoft.com/en-us/windows/msix/package/sign-app-package-using-signtool)
- [Microsoft: Windows App Certification Kit](https://learn.microsoft.com/en-us/windows/uwp/debug-test-perf/windows-app-certification-kit)
