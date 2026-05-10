# editor crate

## 目的
`editor` は原稿用紙スタイルの編集体験を提供する中核 crate です。テキスト編集操作、選択範囲制御、スクロール、Undo/Redo、描画を一体で管理します。

## 役割
- `Editor` による外部 API（カーソル移動・挿入削除・選択・テキスト取得/設定）。
- `EditorState` による編集状態の保持と可視領域キャッシュ管理。
- `EditorCanvas` によるセルグリッド、選択、カーソル、ルビ用余白の描画。
- `settings` と連携した表示パラメータ（列数・行数・セルサイズ）再計算。

## 処理の詳細
1. **入力処理**
   - 非 Vim モード時に矢印キー、削除、コピー/ペーストなどのバインディングを登録する。
   - IME 入力を含むテキスト入力は UTF-16 選択範囲を byte offset に変換して `TextRope` へ反映。

2. **編集トランザクション**
   - 変更前後の `EditorViewState` と差分 (`EditOperation`) を `EditTransaction` として保存。
   - 連続入力をまとめる `PendingTransaction` を使い、Undo/Redo の粒度を制御。

3. **表示計算**
   - ウィンドウサイズと設定値から `visible_columns`, `rows_per_column`, `max_visible_rows` を計算。
   - スクロール位置とドラフト revision をキーに可視セルをキャッシュし、再描画コストを削減。

4. **描画**
   - 背景紙面、列番号、グリッド線、選択ハイライト、カーソルを段階的に描画。
   - グリッドのパスは `GridPathCache` で再利用し、サイズ変更時のみ再構築。

5. **ロープ連携**
   - 実テキストは `rope::TextRope` に保持し、表示セル位置と byte offset の相互変換を委譲。
