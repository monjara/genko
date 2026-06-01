# keymap crate

## 目的
`keymap` はアプリのキーバインド定義を JSON から読み込み、GPUI の `KeyBinding` に変換する crate です。

## 役割
- 既定キーマップを `resources/default-macos.json`、`resources/default-linux.json`、`resources/default-windows.json` で管理。
- XDG ベースディレクトリ配下の `keymap.json` を追加で読み込む。
- JSON 内の action 名を `cx.build_action` で解決し、`KeyBinding` を生成する。

## keymap
- キーマップは Zed 風の `bindings` セクション配列で置く。
- `bindings` は `{ "keystroke": "crate::ActionName" }` の形で、action 名は `actions!` で登録された名前を使う。
- `context` を指定すると、そのコンテキストに一致する場合だけバインドを有効化する。
- ユーザーキーマップは既定キーマップの後に登録されるため、同じ入力はユーザー側が優先される。
