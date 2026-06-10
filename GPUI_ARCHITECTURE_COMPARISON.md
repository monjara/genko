# vendor-example/zed と genko のアーキテクチャ比較

この文書は `vendor-example/zed` とこのリポジトリの `genko` を、主にアプリ構成と GPUI の使い方の観点で比較したものです。

## 全体像

`genko` は単一目的のデスクトップアプリとして、起動時に必要な global state と key binding を登録し、単一の `SoukouApp` を root view として開く構成です。主要な状態は `SoukouApp` と少数の crate-local global に集約されています。

一方、Zed は editor platform として、workspace、project、settings、theme、language、client、session、AI/sidebar、panel、dock、persistence などを多数の crate に分けています。アプリ起動時に `AppState` を組み立て、それを `workspace::AppState` global として共有し、`Workspace` / `MultiWorkspace` が複数 project・pane・dock・sidebar を管理します。

## Workspace 構造

genko:

- `crates/soukou/src/main.rs` が `SoukouApp` を開く。
- `SoukouApp` は `EditorController`、`Workspace`、`TitleBar`、`BottomBar`、認証状態、モーダル、通知を直接所有する。
- `crates/workspace/src/workspace.rs` の `WorkspaceState` は `Global` で、root dir、active file、entries、pane visible などの小さな状態を保持する。
- `Workspace` view はファイル一覧ペインに近く、Zed の「作業空間全体」ではない。

Zed:

- `crates/workspace/src/workspace.rs` の `AppState` が `LanguageRegistry`、`Client`、`UserStore`、`WorkspaceStore`、`Fs`、`NodeRuntime`、`Session` を束ねる。
- `Workspace` は project、pane group、dock、status bar、modal layer、toast、notifications、collaboration state、serialization task などを所有する大きな root context。
- `MultiWorkspace` が複数 `Workspace` と sidebar を統合し、AI/sidebar 有効状態、フォーカス復帰、serialize、project group 切り替えを管理する。

差分:

- genko の `Workspace` は「サイドペイン + ファイルリスト」、Zed の `Workspace` は「アプリ内の作業空間 root」。
- genko では `SoukouApp` が root orchestration を担う。Zed では `Workspace` / `MultiWorkspace` が orchestration の中心。
- genko で Zed 型の `Workspace` に近づけるなら、将来的には `SoukouApp` のうち document/editor/workspace/panel orchestration を専用 root entity に切り出す余地がある。ただし現状規模では `SoukouApp` 集約は読みやすい。

## Global State

genko:

- `Theme`、`AppSettings`、`WorkspaceState`、`VimState` などを直接 `cx.set_global` する。
- global は小さく、ほぼ同期的に読める設定・表示状態が中心。
- `WorkspaceState::global(cx)` / `global_mut(cx)` のような薄い helper を持つ。

Zed:

- `SettingsStore`、`ThemeRegistry`、`ActiveTheme`、`AppState`、`WorkspaceDb`、各 subsystem store など、多層の global/store がある。
- `SettingsStore` は observe 対象で、設定変更に応じて UI や subsystem が反応する。
- `AppState::test` のような test-support 初期化も整備されている。

差分:

- genko は global を「値置き場」として使う傾向が強い。
- Zed は global を「store / registry / service locator」として使い、observe と async task を組み合わせて状態伝播する。
- genko で設定やワークスペースが増える場合は、単純な `Global` 値から store 型へ移行すると Zed に近い拡張性が出る。

## GPUI Entity とイベント

genko:

- `SoukouApp` が `EditorController` と `Workspace` を `cx.new` し、`cx.subscribe` で `EditorEvent` / `WorkspaceEvent` を受ける。
- `EditorController` は `VimController` を包む薄い facade。UI root へは `Render` で `vim_controller.clone()` を返す。
- event は比較的少なく、アプリ root が直接処理する。

Zed:

- `Workspace` は多数の entity を持ち、`subscribe_in`、`observe_global_in`、`WeakEntity`、`Task` を多用する。
- `Project`、`Pane`、`Dock`、`StatusBar`、`ModalLayer`、`ToastLayer` などが相互にイベントを出す。
- `WeakEntity` と task detach/logging を多用し、非同期・window context 付き更新を細かく制御する。

差分:

- genko は entity graph が浅い。
- Zed は entity graph が深く、所有・購読・focus・serialization の境界が明確。
- genko でイベントが増えたら、`SoukouApp` に handler が集中しすぎないよう、domain ごとの controller/store に寄せるとよい。

## Render と UI 構築

genko:

- `div()` と Tailwind 風 fluent API を直接使う。
- `MenuBar`、`TextInput`、`Tooltip` など少数の独自 UI 部品を `crates/ui` に置く。
- 見た目は `Theme::global(cx)` を直接参照して色を決める。
- `RenderOnce` は一時的な popover/modal/toolbars に使われている。

Zed:

- `ui` crate に Button、IconButton、Label、Popover、ContextMenu、Modal、List、Tab、Toggle、Keybinding など大量の再利用部品がある。
- `ui::prelude::*`、style token、component traits により、アプリ側は高水準コンポーネントを組み合わせる。
- `Workspace` rendering は pane/dock/sidebar/status bar の合成で、`canvas`、`deferred`、absolute layer、client-side decorations を細かく使う。

差分:

- genko は GPUI primitive 直書きが多く、局所的には分かりやすい。
- Zed は design system 化されており、UI 一貫性と再利用性が高い。
- genko で同じ形のボタン、chip、toggle、modal action が増えてきたら、Zed のように `ui` crate へ小さな component と style token を寄せる価値がある。

## 入力と TextInput

genko:

- `crates/ui/src/text_input.rs` に GPUI の `ElementInputHandler` まで含む独自 text input がある。
- `SoukouTextInput` key context を使い、`Backspace`、`Left`、`Paste` などを直接 action として登録する。
- 縦書き対応のため、layout、selection、IME anchor、paint を独自に扱う。

Zed:

- 通常の入力は editor/buffer infrastructure を使う。
- `ui_input` crate は `ErasedEditor` trait を定義し、form-like input でも editor 実装を抽象化して使う。
- 入力部品は editor model と統合され、buffer edited / blurred などの event を購読できる。

差分:

- genko は軽量な独自 input。縦書きやルビ編集など目的特化の実装になっている。
- Zed は editor を基盤にして入力欄も抽象化する。
- genko の text input が複数用途に広がる場合、`ErasedEditor` 的な trait で「入力欄として必要な操作」を抽象化すると依存方向を整理しやすい。

## Actions と Keymap

genko:

- `actions!` は crate ごとに少数定義される。
- keymap は `crates/keymap/resources/default-*.json` を読み、`cx.build_action` と `KeyBinding::load` で解決する。
- user keymap は XDG config の `keymap.json` を追加で読む。
- 失敗時は `LoadedKeyBindings { error }` で UI 通知に回す。

Zed:

- action 数が非常に多く、`zed_actions` や各 crate の action が統合される。
- `settings::KeymapFile` が JSONC、schema、unbinding、action sequence、partial failure、metadata、validator を扱う。
- keymap 編集 UI や documentation との連携まで考慮されている。

差分:

- genko の keymap loader は単純で十分だが、JSON comment、unbinding、partial load、schema はない。
- 将来的に keymap customization を前面に出すなら、Zed の `KeymapFileLoadResult` のように「読み込める binding は残し、エラーを集約して表示する」設計が参考になる。

## Settings と Theme

genko:

- `AppSettings` は serde 可能な plain struct。
- `settings::init` でファイルから読み、`AppSettings` global に置く。
- 設定画面の操作は `AppSettings::global_mut(cx)` を直接更新し、その場で `save()` する。
- `Theme` は default JSON を読み、色値を getter で返す単一 theme。

Zed:

- `SettingsStore` が default/user/project/profile/OS/release-channel override を扱う。
- `settings_content` と schema 生成を持つ。
- theme は `ThemeRegistry`、`ActiveTheme`、theme settings、icon theme などに分かれる。
- `observe_global` で設定変更が UI や subsystem に反映される。

差分:

- genko は単一設定ファイル + 単一テーマ。
- Zed は layered settings + registry。
- genko で project-local settings や theme 切り替えを入れるなら、現在の `AppSettings` global を `SettingsStore` 的な entity/global に変えるタイミング。

## Window とタイトルバー

genko:

- `gpui_platform::application()` を使い、main window と settings window を直接 `cx.open_window` する。
- client-side decoration を `title_bar` crate でラップする。
- macOS/Linux/Windows の差分は主に `TitleBar` 内で吸収する。

Zed:

- `Application::with_platform(...)`、quit mode、single instance、CLI mode、crash handler、open listener、session restore を含む。
- `build_window_options` は `AppState` に入り、workspace restore と連携する。
- workspace/multi-workspace は window close 時に unsaved item、workspace serialization、focus restore を処理する。

差分:

- genko は window lifecycle が単純。
- Zed は CLI/open URL/session restore/crash recovery まで含む。
- genko でも複数 window や session restore を入れるなら、`main.rs` から window lifecycle を切り出す必要が出る。

## Async / Task / Error Handling

genko:

- auth restore、URL callback、file dialog / export などで `cx.spawn` や async update を使う。
- 失敗は `show_error_modal` / notification に集約する傾向。
- 一部は `eprintln!` で fallback する。

Zed:

- `TaskExt`、`detach_and_log_err`、`ResultExt::log_err`、background executor、Tokio integration を広く使う。
- async task が多く、window/entity lifetime に合わせて `WeakEntity` や `cx.spawn_in` を使い分ける。
- エラーは toast、notification、prompt、log、telemetry に流れる。

差分:

- genko は async の面積がまだ小さい。
- 非同期処理が増えた場合は、Zed のように「detach する task は必ず log/error path を持つ」ルールを徹底するとよい。

## テスト

genko:

- ロジックテストが中心。
- 直近で `gpui::test` と `TestAppContext` を使う UI テストを追加し始めた。
- `gpui` の `test-support` feature を dev-dependency として有効化している。

Zed:

- `#[gpui::test]`、`TestAppContext`、`VisualTestContext`、fake fs / fake http / test AppState が広く使われる。
- 大きな subsystem には test helper context がある。例: `AppState::test`、Vim の `VimTestContext`。
- UI 操作だけでなく、workspace/project/file system/network を fake 化して統合的に検証する。

差分:

- genko はまず component 単位の GPUI テストが妥当。
- editor/workspace/file IO を含む統合テストを増やすなら、Zed の `AppState::test` に近い `SoukouTestContext` を作ると初期化重複を減らせる。

## genko に取り込む価値が高いパターン

1. Test helper の集約
   - `theme::init`、`settings` default、`WorkspaceState::init`、`editor::init` などをまとめる `init_test_app(cx)` を用意する。

2. Store 化
   - `AppSettings` や `WorkspaceState` が複雑化したら、plain global から store entity/global に寄せる。

3. UI component 化
   - toggle、chip、toolbar button、modal button など重複が増えたら `crates/ui` に component と style token を追加する。

4. Partial failure の keymap/settings
   - user config は「全部失敗」より「読める部分を使い、エラーを UI に出す」方が使いやすい。Zed の keymap loader は参考になる。

5. Entity boundary の整理
   - `SoukouApp` の handler が増えたら、document controller、workspace controller、modal manager のように責務を分ける。

## genko ではまだ避けた方がよい Zed パターン

- `Workspace` / `MultiWorkspace` 級の汎用 pane/dock architecture。
  現状の genko には過剰で、読むコストが大きい。

- settings schema / profile / project override のフル実装。
  project-local 設定や拡張機構が必要になるまでは plain `AppSettings` で十分。

- editor を全入力欄の基盤にする構成。
  genko の `TextInput` は縦書き・ルビ編集に特化しており、今は独自実装の方が目的に合う。

- telemetry / collaboration / remote / session restore を前提にした task graph。
  機能要求が出るまでは導入しない方がよい。

## まとめ

Zed は「拡張可能な editor platform」、genko は「縦書き原稿作成に特化した単一アプリ」です。GPUI の基本的な使い方、つまり `Entity`、`Context`、`Render`、`actions!`、`cx.subscribe`、`cx.notify`、`cx.spawn` は共通していますが、Zed はそれらを store、registry、pane/dock、multi-workspace、settings schema と組み合わせて大規模化しています。

genko は今の段階では、Zed の大きな構造をそのまま移植するより、以下の順で部分的に取り込むのが現実的です。

1. test helper と GPUI UI テストの拡充
2. 重複 UI の component 化
3. settings/keymap の partial failure と UI feedback 強化
4. `SoukouApp` に集中している orchestration の段階的分割
5. 複数 window / session restore が必要になった時点で window lifecycle を再設計
