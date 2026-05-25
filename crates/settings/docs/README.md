# settings crate

## 目的
`settings` はアプリ設定の定義・正規化・保存/読込を担う crate です。UI と編集ロジックが共通で参照するグローバル設定を提供します。

## 役割
- `AppSettings` 構造体で表示・編集挙動の設定値を管理。
- XDG ベースディレクトリ配下の `settings.json` をロード/保存。
- XDG ベースディレクトリ配下の `keymap.json` をロードし、未配置時は同梱の `default_keymap.json` を使う。
- 値の範囲制約（セルサイズ、列あたり文字数）の正規化。
- `gpui::Global` としてアプリ全域から参照可能にする。

## 処理の詳細
1. **設定項目**
   - グリッド表示、ぶら下げ句読点、列番号表示モード。
   - セルサイズ、`rows_per_column`（自動 or 固定）。
   - Vim モード有効/無効。
   - `keymap` によるキーバインド上書き。

2. **読み込みフロー**
   - `settings::init` で `AppSettings::load` を実行。
   - `settings.json` は一般設定、`keymap.json` はキーマップとして別々に読む。
   - `keymap.json` が未存在・読込失敗・JSON 不正時は同梱の `default_keymap.json` へフォールバックする。
   - 旧形式の `settings.json.keymap` は互換用フォールバックとしてのみ読む。
   - 読込成功時も `normalized()` で制約範囲へ丸める。

3. **保存フロー**
   - `save()` で保存先ディレクトリを作成し `settings.json` を pretty JSON で書き出す。
   - キーマップは `settings.json` へ保存せず、`keymap.json` を手動配置して上書きする。
   - I/O/serialize エラーはユーザー表示向け文字列で返却。

4. **検証**
   - unit test で欠損ファイル、値の clamp、保存内容などを検証。

## keymap
- 既定キーマップは [`resources/default_keymap.json`](../resources/default_keymap.json) に置く。
- ユーザー上書きは設定ディレクトリの `keymap.json` に `{ "id": "...", "keystroke": "..." }` の配列で置く。
- `id` はコード側で定義した安定 ID で、`app.open_file.ctrl` や `editor.copy.mac` のように扱う。
- 現在は既存バインドの上書きに限定し、任意アクション追加やコンテキスト編集は行わない。
- 空要素は無視し、同じ `id` が重複した場合は最後の要素を採用する。
- `keymap.json` に存在しない `id` は既定キーマップ側の値を使う。
