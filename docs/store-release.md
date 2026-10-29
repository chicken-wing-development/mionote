# Store版とポータブル版の配布

アプリ本体1.0.0、Windows 11、ARM64とx64。UIは日本語。
Store用MSIXの初回パッケージバージョンは1.0.0.0で、本体のバージョンとは独立して管理する。

| 項目 | Microsoft Store用 | ポータブル用 |
| --- | --- | --- |
| 配布物 | ARM64／x64のMSIXと両方を含むMSIXbundle | CPU別ZIPとEXE |
| 設定 | ApplicationData.LocalFolder（LocalState） | EXEの隣のdata/settings.ini |
| 任意の診断ログ | LocalState/mio-note.log | data/mio-note.log |
| 更新 | Storeが管理 | フォルダー内の配布ファイルを利用者が更新 |
| 追加実行環境 | Windows標準APIのみ | Windows標準APIのみ |
| 署名 | 提出用MSIXは未署名。審査後Storeが署名 | 現在は未署名。公開経路に応じAuthenticode署名を付ける |

## Storeの本人性

`distribution/store/identity.json` は利用者から提供された次の値を含む。

- Package/Identity/Name: `5D6DC947.MioNote`
- Package/Identity/Publisher: `CN=D0411010-4A66-49C6-8501-3C08B3C6E4C7`
- Package/Properties/PublisherDisplayName: `テバ開発`

Name・PublisherはPartner Centerの表示どおりに、大文字小文字も含めて一致させる。
表示名はMioNote。予約した表示名が異なる場合はDisplayNameも合わせる。
更新ごとにPackageVersionを増やす。4番目の数字はStore用として0を維持し、先頭は1以上にする。
同一バージョンのARM64とx64を1つのbundleへ含める。

## ビルド

Rust／MSVC／Windows SDKの開発環境を使う。通常のアプリ利用者には不要。

```powershell
.\package.ps1 -Architecture arm64
.\package.ps1 -Architecture x64
.\package-store.ps1
.\package-source.ps1
python scripts/verify-packages.py
```

Store作成にはMicrosoft署名のあるMakeAppx.exeとMakePri.exeが必要。PATHか `-SdkBin` で指定する。
この作業環境では隣接するtools内のWindows SDK BuildToolsも検出する。
ARM64ホストでx64ビルドを行う場合、この環境ではx64ホスト用Rustとx64のMSVCライブラリを用いる。
他の環境では対応CPU向けDeveloper PowerShellとRustターゲットを設定する。
`--features store` を付けたEXEをポータブル版へ混入させない。Store版の設定APIにはパッケージの識別情報が必要。

MSIXの作成はスキーマ・意味検証を有効にしたMakeAppxで行う。検証を省く `/nv` は使わない。
SDKツールの署名、PEのCPU、ASLR・DEP・CFG、System32限定のDLL読み込み、共有CRT非依存も確認する。
マニフェストに宣言する権限は `runFullTrust` のみ。既存のWin32／Windows Inkに必要であり、管理者昇格は要求しない。
この構成はAppContainerではなく、権限一覧だけで通信や全ファイルへのアクセスを遮断するものではない。

## 提出前に利用者が完了する工程

1. プライバシーポリシーをPartner Centerへ登録する。現行のMicrosoft公式手順ではURLの指定に加えて本文の直接登録にも対応している。本文入力欄がある場合は `distribution/PRIVACY.txt` の全文を登録でき、自分のサイトへの公開は必須ではない。URLで登録する場合は `distribution/privacy.html` をHTTPSサイトなどに公開して指定する。Win32／Desktop Bridgeアプリにはポリシー自体が必要で、配布物への同梱だけでStore側の登録を省略する手順にはしない。サポート連絡先はメールアドレスまたはWebページのURLを登録でき、PC向けでは推奨項目。文書内の問い合わせ案内と一致させる。
2. 署名済みパッケージのインストール・起動・設定復元・保存／書き出し・更新・アンインストールと、ポータブル版との設定分離を確認する。2026-10-27にこのARM64端末でローカルの検証署名を使って確認した（x64はエミュレーション）。0.12.1の[実動作確認と範囲](store-runtime-validation-20261027.md)を参照。1.0.0ではアイコン・バージョン情報・コピーライト画面の[差分確認](release-validation-1.0.0-20261027.md)を実施。Store経由の配布と実ペンの確認は別途行う。
3. Windows SDKのWindows App Certification Kit（WACK）で提出物を検査し、レポートを保存する。最終1.0.0のARM64・x64 MSIXとbundleは各24項目がPASS、部分実行フラグFALSE。[1.0.0の実行結果](release-validation-1.0.0-20261027.md)と[機械可読レポート](wack-results-1.0.0-20261027.json)を参照。導入経緯・CLIの再実行手順は[既存のWACK報告](wack-validation-20261027.md)に記載（再検査時は1.0.0のファイル名を指定する）。提出物を変更したら再検査する。
4. Partner CenterへMSIXbundleを登録し、下記の審査用説明、対応するスクリーンショット、年齢レーティング、カテゴリ、必要なサポート情報を揃える。公開の確定は利用者が行う。

MSIXのStore提出にはCA証明書の購入を前提としない。Storeが審査後に署名し直す。
Store外でMSIXをインストールする場合は、発行元と一致する信頼された署名が別途必要。
このスクリプトは証明書ストア、開発者モード、OSのセキュリティ設定を変更しない。

## ポータブル版

ZIPを展開し、CPUに合ったMioNote.exeを実行する。mio-note.portableを一緒に保持する。
EXEだけをコピーすると従来のプロファイル保存に戻るため、設定も持ち運ぶ場合はフォルダー全体をコピーする。
書き込み不可の媒体では設定の保存に失敗する。別の場所への自動退避はしない。
dataは利用者の情報なので、更新時に削除したり、配布ZIPへ取り込んだりしない。

認証局などの証明書で署名したEXEを用いる場合は、同じ本体バージョン・CPUのEXEを指定して検証付きで再パッケージ化できる。

```powershell
.\package.ps1 -Architecture arm64 -SignedExecutable 'C:\release\MioNote.exe' -RequireSignature
```

自己署名を利用者の端末へ自動信頼させたり、警告やSmartScreenを無効化する処理は追加しない。

## 審査用説明の下書き

MioNoteはWindows Inkを使う日本語の手書きノートです。アカウント、ネット接続、追加ランタイムは不要です。
マウスの左ドラッグでも筆記できます。対応ペンで筆圧入力と消しゴムを利用できます。
保存はCtrl+S、読み込みはCtrl+O、PDFはCtrl+E、PNG／SVGは書き出しメニューから利用します。
A4・A5・JIS B4・JIS B5、縦横、ページ追加／削除、拡大縮小に対応します。
runFullTrustは既存のWin32ウィンドウとWindows Inkを利用するために必要です。管理者権限、サービス、自動起動は使用しません。
アプリ本体に通信、広告、アクセス解析、自動更新の実装はありません。設定はアプリ専用のLocalStateへ保存します。
ノートは利用者が指定した場所へ保存し、内容を発行元へ送信しません。
Undoとページの並べ替え、PDF／画像の取り込みは未実装です。個人のノートを審査用サンプルへ含めないでください。

## 一次資料

- [MSIXアプリのプライバシーポリシーとサポート情報（URLまたは本文登録）](https://learn.microsoft.com/ja-jp/windows/apps/publish/publish-your-app/msix/support-info)
- [Microsoft StoreのMSIXパッケージ要件・署名](https://learn.microsoft.com/en-us/windows/apps/publish/publish-your-app/msix/app-package-requirements)
- [Microsoft Store Policies](https://learn.microsoft.com/en-us/windows/apps/publish/store-policies) 10.2／10.5／10.6
- [MSIX手動パッケージ化](https://learn.microsoft.com/en-us/windows/msix/desktop/desktop-to-uwp-manual-conversion)
