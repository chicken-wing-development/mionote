# Store版・ポータブル版の追加レビュー — 2026-10-27

対象はMioNote 0.12.0、Windows 11のARM64／x64向け。前回の[入力境界レビュー](security-review-20261027.md)を継続する。

## 修正した点

- 一時ファイルを親フォルダーの既定ACLだけで作ると、個別に保護した保存先より広い権限になり得た。保存先のDACLを読み取り、一時ファイル作成時から適用する。書き込み前に実際のDACLも読み戻し、一致しなければ保存を中止する。ACL取得・適用が失敗した場合に権限を緩めて続行しない。
- 既存ファイルがEFSで暗号化されている場合、一時ファイルにも暗号化属性を指定し、実際の属性を確認してから内容を書く。保持できなければ既存ファイルを残して失敗する。新たな独自暗号方式は導入しない。
- Storeのインストール先は読み取り専用。Storeビルドの設定と任意の診断ログはApplicationData.LocalFolderへ移した。ポータブル版は明示的なマーカーで隣のdataへ保存し、失敗時にユーザープロファイルへ黙って切り替えない。
- 診断ログを1 MiBに制限し、通常の書き出しログから保存先パスを除いた。エラーメッセージに含まれるパスは残り得るためプライバシー文書に記載した。
- WindowsのISF解析前に、raw ISFのヘッダー、32ビット可変長整数、宣言長と実長を照合。GIF／Base64のラッパー、整数オーバーフロー、末尾への追加データを拒否する。
- 起動引数を1つのノートに限定し、オプション・別拡張子・デバイス名・ADSを拒否。初期化通知が重複してもノートを再読み込みしない。
- 配布ZIPは毎回新しい作業フォルダーから作る。以前の配布フォルダーに利用者が置いたノート、設定、ログが混入しない。
- ARM64とx64の両方で静的CRT・ASLR・DEP・CFG・System32限定の依存DLL読み込みを検証する。

## MSIXの権限と本人性

正式IDは利用者から提供されたものをidentity.jsonへ設定した。秘密鍵や証明書は含まない。
runFullTrustのみを宣言し、管理者昇格、サービス、自動起動、広範なファイルシステムアクセスの追加権限は要求しない。
full trustのWin32アプリはAppContainerではない。internetClientが未宣言でもOSが全ての通信を遮断するという意味にはならない。
アプリ本体には通信・広告・解析SDK・外部コード取得・自動更新機能がなく、更新はStoreに任せる。

## 検証と残る工程

最終コードでARM64とx64の各36件のテストが成功した。x64の実行環境はWindows on Armのエミュレーションで、実物のIntel／AMD端末での試験ではない。
両CPU・通常／Store機能の全4構成でClippy（警告をエラー扱い）を通過し、rustfmtも確認した。
個別DACLを持つ保存先について、一時ファイルと置換後のDACLが維持されることを確認した。
Windowsが正規化する所有者などの記述子メタデータは比較せず、実際のACLと継承保護を比較する。
ポータブル版は両CPUで起動し、専用dataへの設定保存を確認した。ARM64では再起動後の設定復元も画面で確認した。通常起動で診断ログは作られなかった。
依存45件のOSV照合は検出0件。Cargo.lockのハッシュと結果を同梱した。
MakeAppxのスキーマ・意味検証を有効にし、両CPUのMSIXとbundleを作成した。
配布内容・PEのCPUと保護フラグ・ZIPのCRC・MSIXのブロックハッシュ・ソースのSHA-256はscripts/verify-packages.pyで確認する。

権限保全の対象はDACLであり、監査用SACL・所有者・全ての代替ストリームや属性の完全な複製を保証するものではない。保存先フォルダーは利用者が信頼する場所を前提とする。
Windows InkのISF解析は依然として同一プロセス内のOS実装であり、外側の形式検証だけでOSの解析バグや時間・メモリーを完全には制限できない。
EFS実機、外付けFAT/exFAT、ネットワーク共有、別ユーザーのACL、実物のx64ペン端末については追加の受け入れ確認が必要。
Store提出前にWindows App Certification Kitと、署名済みパッケージのインストール・更新・削除・設定の分離を確認する。
この環境にWACKはなく、これらのパッケージ導入試験とStore版LocalStateの実動作確認は未実施。Storeへのアップロードや公開も行っていない。
今回の作業はStore審査の合格や特定規格への認証を保証するものではない。

## 根拠

- [MSIXの手動パッケージ化](https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-manual-conversion)
- [Storeのパッケージ要件・署名](https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msix/app-package-requirements)
- [Store Policies 10.2、10.5、10.6](https://learn.microsoft.com/en-us/windows/apps/publish/store-policies)
- [WindowsのCreateFileWとセキュリティ属性](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-createfilew)
- [Microsoft WPFのraw ISFヘッダー処理](https://github.com/dotnet/wpf/blob/main/src/Microsoft.DotNet.Wpf/src/PresentationCore/MS/internal/Ink/InkSerializedFormat/InkSerializer.cs)
