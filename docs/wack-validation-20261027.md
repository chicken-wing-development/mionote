# Store版のWACK検証 — 2026-10-27

最終MioNote 0.12.1（Storeパッケージ1.0.0.0）の未署名の提出物を、Windows App Certification Kit 10.0.28000.2957で検査した。3つのレポートすべてが `OVERALL_RESULT="PASS"`、`PARTIAL_RUN="FALSE"`。各24項目がPASSで、メッセージ欄に警告・失敗はなかった。

| 検査対象 | 結果 | PASS項目数 | XML原本／HTML表示 |
| --- | --- | --- | --- |
| ARM64・x64を含むMSIXbundle | PASS | 24 | [XML](../target/wack-0.12.1-final-20261027/bundle-report.xml)／[HTML](../target/wack-0.12.1-final-20261027/bundle-report.html) |
| ARM64 MSIX | PASS | 24 | [XML](../target/wack-0.12.1-final-20261027/arm64-report.xml)／[HTML](../target/wack-0.12.1-final-20261027/arm64-report.html) |
| x64 MSIX | PASS | 24 | [XML](../target/wack-0.12.1-final-20261027/x64-report.xml)／[HTML](../target/wack-0.12.1-final-20261027/x64-report.html) |

XML原本と詳細ログは、この検証端末の `target/wack-0.12.1-final-20261027` に保存している。HTMLはSDK付属のMicrosoftのXSLと日本語文字列から生成した表示用ファイル。個人の端末名やローカルパスを含む原本・ログは公開用ソースZIPへ入れず、項目別結果とSHA-256を記録した [wack-results-0.12.1-20261027.json](wack-results-0.12.1-20261027.json) を同梱する。

## 検査環境と導入

- ホスト: Windows 11 Home ARM64、10.0.26300.0。
- WACK: 10.0.28000.2957、x86版をWindows on Armのエミュレーションで実行。
- 導入先: `C:\Program Files (x86)\Windows Kits\10\App Certification Kit\appcert.exe`。
- Microsoft公式のWindows SDKセットアップと、レイアウトに含まれるMSIの有効なMicrosoft Authenticode署名を確認した。
- SDKセットアップの `OptionId.WindowsSoftwareLogoToolkit` を導入した。ARM64ホストではWACK本体が自動選択されなかったため、同じ公式レイアウトのx86版Main・OnecoreUAP・Native ComponentsのMSIを追加した。
- TAEFの親ディレクトリ用x86機能 `fea721edbfa1174f728a639dbb8a6660b1` も、Main MSIの `ADDLOCAL` で追加した。MSIやテスト設定の編集は行っていない。
- セットアップと追加MSI・TAEF機能の終了コードはすべて0。再起動は要求されなかった。

取得したMicrosoft公式の `winsdksetup.exe` はバージョン10.1.28000.2957、SHA-256は `e8e264aca8bc34abdf592352bf09f986a85cdb96db0efba405ca9e149181fe61`。ダウンロードと導入のログは `tools/downloads/wack-10.0.28000.2957` と本プロジェクトの `target/wack-20261027` にある。

最初の展開のみの状態ではCOM部品の未登録で停止し、導入直後はTAEFの親ディレクトリ不足で停止した。これらは検査環境の準備段階のエラーとして別のログへ保存した。上記の3つの最終レポートは、不足部品の導入後に `appcert reset` から再実行した結果で、各終了コードは0。

## 確認できた範囲

WACKはアプリを `Centennial` と判定し、インストールせずにパッケージからファイルを取り出す検査を行った。マニフェスト、リソース、ブランド画像、デバッグ構成、UAC実行レベル、禁止ファイル、秘密鍵の混入、レジストリ宣言、ファイル関連付け、エンタープライズ機能、メタデータなどの適用対象が成功した。手動でテストを除外する `-testid` は指定していない。

bundleのレポートで選択されたペイロードはx64だったため、ARM64 MSIXも個別に検査した。個別レポートのマニフェストはそれぞれarm64／x64で、WACKの実行ファイル認識も `pe64_arm64`／`pe64_amd64` であることを確認した。

Storeへの登録・公開は行っていない。WACK自身ではパッケージのインストールや起動は行われていないが、別途[署名済みビルドの動作確認](store-runtime-validation-20261027.md)を実施し、導入・更新・削除、LocalState設定の保存・復元、ノート保存と書き出しを確認した。Intel／AMD実機上での検査とStore経由の配布は未実施。WACKのPASSと実施したローカル試験の範囲を区別する。

WACKは未署名の原本で実施し、検査対象3ファイルは検査前後で同じSHA-256を維持した。動作試験では別コピーへ一時証明書で署名し、確認後に証明書の信頼登録と秘密鍵を削除した。Rootへの登録や開発者モードの有効化は行っていない。同梱の0.12.0追加セキュリティレビューはWACK導入前の記録であり、その後の結果は本書と動作確認書で管理する。

## 検査した提出物のSHA-256

| ファイル | SHA-256 |
| --- | --- |
| MioNote-0.12.1-store.msixbundle | `ad094368b34132c5e3729a1a6b896dcd9ddc4d57521bbe3dc5bd859cdde45518` |
| MioNote-0.12.1-store-arm64.msix | `c9ad149bc45ac229bb2ff2b3cffed9e09fff13e9f5473e49f94aa2d82143450a` |
| MioNote-0.12.1-store-x64.msix | `4f008db12bb8f2b1dfe293c540b01763e1b2645e89e9f3cd25ca71fe2e4306ca` |

## 再実行

管理者として開いたPowerShellで、本プロジェクトのルートから実行する。WACKはアクティブなユーザーセッション内で実行する必要がある。`reset` はWACKの作業中データをリセットするため、以前のレポートは先に別のフォルダーへ保存する。

```powershell
$wack = 'C:\Program Files (x86)\Windows Kits\10\App Certification Kit\appcert.exe'
$project = (Get-Location).Path
$reports = Join-Path $project ('target/wack-' + [Guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Path $reports | Out-Null

foreach ($suffix in @('', '-arm64', '-x64')) {
    $file = if ($suffix) { "MioNote-0.12.1-store$suffix.msix" } else { 'MioNote-0.12.1-store.msixbundle' }
    $label = if ($suffix) { $suffix.Substring(1) } else { 'bundle' }
    $report = Join-Path $reports "$label-report.xml"
    & $wack reset
    if ($LASTEXITCODE -ne 0) { throw 'WACK reset failed.' }
    & $wack test -appxpackagepath (Join-Path $project "dist/$file") -reportoutputpath $report
    if ($LASTEXITCODE -ne 0) { throw "WACK execution failed: $label" }
    $result = [xml](Get-Content -LiteralPath $report -Raw)
    if ($result.REPORT.OVERALL_RESULT -ne 'PASS' -or $result.REPORT.PARTIAL_RUN -ne 'FALSE') {
        throw "Review the WACK report: $report"
    }
}
```

ビルドや再パッケージ化で提出物が変わったら、新しいファイルに対して再実行する。

## 以前の検査記録

0.12.0も同日に3配布物が各24項目PASSだった。[当時の結果JSON](wack-results-20261027.json)と、この端末の `target/wack-20261027` の原本を保持している。フォーカス処理の最終調整前に実施した0.12.1の原本は `target/wack-0.12.1-20261027` に残し、上記の最終提出物の結果と区別している。

## 一次資料

- [Microsoft: Windows App Certification Kitの導入とCLIでの実行](https://learn.microsoft.com/en-us/windows/uwp/debug-test-perf/windows-app-certification-kit)
- [Microsoft: Windows SDKのダウンロード](https://learn.microsoft.com/en-us/windows/apps/windows-sdk/downloads)
- [Microsoft: Windows on Armのエミュレーション](https://learn.microsoft.com/en-us/windows/arm/overview)
