# vim crate

## 目的
`vim` はエディタ上で Vim 風モーダル編集を提供する crate です。通常入力モードを置き換え、Normal/Insert/Visual/Visual Block の操作体系を実現します。

## 役割
- キーバインド定義 (`bindings`) と状態遷移 (`state`)。
- 演算子（d/c/y）とモーション/テキストオブジェクトを組み合わせた編集。
- レジスタ保持とペースト、繰り返し (`.`) の再実行。
- ブロック選択・ブロック挿入/削除 (`block`)。
- 単語境界解析や引用符/括弧オブジェクト解決 (`text_objects`)。

## 処理の詳細
1. **モード管理**
   - `VimState` が現在モード、Visual アンカー、保留中オペレータを保持。
   - Insert へ入ると `Editor` のテキスト入力を有効化、Normal へ戻ると無効化。

2. **オペレータ実行**
   - `d`, `c`, `y` 入力後に motion/text object を待ち、対象 range を計算して編集。
   - change 操作は insert セッション開始と連携し、確定時に repeat 情報を記録。

3. **テキストオブジェクト/モーション**
   - `iw`, `aw`, `i"`, `a"`, `i(` などを `resolve_text_object_range` で解決。
   - `w`, `W`, `e` を `resolve_motion_target/range` で解決。
   - 日本語単語境界は Lindera を使う実装を持ち、辞書読み込み結果を `OnceLock` でキャッシュ。

4. **ブロック編集**
   - Visual Block 範囲から byte range 群を導出して一括削除/変更/貼り付け。
   - ブロックレジスタは行数・列数とセル配列を持ち、形状維持した再貼り付けに利用。

5. **テスト**
   - text object と motion の境界解決を中心に unit test を整備し、特に空白処理や括弧ネストの挙動を確認。
