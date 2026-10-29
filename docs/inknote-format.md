# .inknote ファイル形式（version 2）

0.7.0以降は固定用紙・複数ページ用の形式です。拡張子は.inknoteです。
0.6.0以前の単一ページ形式（version 1）は読み込まず、変換も行いません。

ZIPには document.json と、ページごとの pages/0.isf、pages/1.isf ... を格納します。
各エントリーはStore方式（無圧縮）です。ISF自体が筆跡を圧縮します。
Deflate方式に再圧縮した外部ファイルには対応しません。

## document.json

UTF-8のJSONです。A4縦とJIS B5横の例：

```json
{
  "format": "MioNote",
  "version": 2,
  "active_page": 0,
  "pages": [
    {
      "paper": { "size": "A4", "orientation": "portrait" },
      "rule_spacing": "standard"
    },
    {
      "paper": { "size": "B5", "orientation": "landscape" },
      "rule_spacing": "wide"
    }
  ]
}
```

- formatはMioNote、versionは整数2です。
- active_pageは0から始まる表示ページ番号です。ページ移動だけでは変更状態にせず、手動保存時の表示ページを記録します。
- pagesは表示・出力順の配列で、1～1000ページです。
- paper.sizeはA4 / A5 / B4 / B5、orientationはportrait / landscapeです。
- rule_spacingはnarrow / standard / wide（28 / 36 / 44 DIP）です。

| 用紙 | 縦向きの幅×高さ |
| --- | --- |
| A4 | 210×297 mm |
| A5 | 148×210 mm |
| B4（JIS） | 257×364 mm |
| B5（JIS） | 182×257 mm |

任意の幅・高さは受け付けません。横向きでは幅と高さが入れ替わります。

## ISFと座標

pages/0.isf以降はWindows InkのISFです。空白ページも空のISFを保存します。
筆点・筆圧・描画属性・変換行列を保持します。
保存座標は96 DPIのDIP（1 DIP = 1/96 inch）で、実寸mmへはDIP×25.4/96で換算します。
表示中の筆跡は表示倍率を使い、ワーカーで複製した筆跡だけを保存座標へ正規化します。
ズーム・スクロール位置はファイルへ保存しません。スクロールはページのウィンドウを移動し、筆跡座標へ平行移動を加えません。
ウィンドウサイズやモニターのDPIは保存形式に含めません。

## 入出力と検証

- ZIP全体は最大128 MiB、エントリー数は最大1001です。
- document.jsonは最大1 MiB、ISFは1ページ最大8 MiB・全ページの合計最大64 MiBです。
- 必須エントリーを名前で読み、メモリー内で処理します。ZIPのパスをディスクへ展開しません。
- 旧形式、不正な用紙・向き・ページ番号、必須ページの欠落、壊れたISFはエラーです。
- 全ページのISFを復元・検証してから、表示中のノート全体を置き換えます。
- 保存は宛先と同じフォルダーの一時ファイルを完成・同期してから置き換えます。失敗時は以前のノートを保持します。
- 0.11.0以降はStored方式に限定し、実際のエントリー数を制限しながら読みます。ローカルヘッダーと中央ディレクトリーの整合、CRC、重複・想定外の名前も検証します。
- 筆点・座標などの制限は[セキュリティレビュー](security-review-20261027.md)を参照してください。

実装はsrc/note.rs、src/ink_io.rs、src/ink.rsです。
