# rope crate

## 目的
`rope` は大きなテキストを効率よく編集・参照するためのデータ構造 crate です。原稿用紙表示に必要な「表示セル」概念を持ち、文字列編集と描画用インデックス変換を提供します。

## 役割
- `TextRope` による immutable 風ノード分割ベースの編集（範囲置換/追記最適化）。
- Unicode grapheme を考慮した走査とオフセット変換。
- 原稿用紙表示向けのセル分解 (`CellText`) と可視範囲抽出。
- UTF-16/byte/grapheme/display-cell の相互変換 API 提供。

## 処理の詳細
1. **構築と設定**
   - `rows_per_column` と `hanging_punctuation` を持つロープを生成。
   - 設定変更時はノードのセル進み量（表示幅相当）を再計算。

2. **編集**
   - `replace_range` / `replace_range_owned` は境界を文字境界へ正規化して安全に置換。
   - ノード分割 (`split`) と結合 (`concat`) で局所更新し、全体再構築を回避。
   - 末尾追記は leaf サイズ条件を満たす場合に高速パスで処理。

3. **参照と可視化**
   - `slice` で byte range 部分文字列を取得。
   - `visible_cells` で指定セル範囲の描画情報を返却（logical index、文字列、元 byte range）。

4. **インデックス変換**
   - `byte <-> utf16`, `byte <-> grapheme`, `byte <-> display_cell` を提供。
   - エディタ側はこの変換を使って IME と描画座標を橋渡しする。
