use std::collections::HashMap;
use std::path::PathBuf;

use js_bridge_bindgen::{Config, generate_code, parse_from_str};

fn assert_valid_rust_syntax(code: &str, label: &str) {
    if let Err(e) = syn::parse_file(code) {
        panic!("[{label}] 生成的代码不是合法 Rust 语法:\n{e}\n\n--- 生成的代码 ---\n{code}");
    }
}

fn parse_synthetic(ts: &str) -> js_bridge_bindgen::ParsedModule {
    parse_from_str(ts, std::path::Path::new("test.ts")).expect("parse_from_str 失败")
}

fn synthetic_config(factory: &str) -> Config {
    Config {
        entry: PathBuf::from("test.ts"),
        out_dir: PathBuf::from("/tmp"),
        global_factory_name: factory.into(),
        ..Default::default()
    }
}

#[test]
fn test_synthetic_full_pipeline() {
    let ts = r#"
export interface Config {
  name: string
  count: number
}

export interface Widget {
  greet: (msg: string) => void
  getValue: () => number
  destroy: () => void
}

export function createWidget(config: Config): Widget {
  return {
    greet: (msg) => console.log(msg),
    getValue: () => 42,
    destroy: () => {},
  }
}

declare global {
  interface Window {
    __factory: {
      createWidget: (config: Config) => Widget
    }
    __widget?: Widget
  }
}
"#;

    let module = parse_synthetic(ts);
    let config = synthetic_config("__factory");
    let code = generate_code(&module, &config);

    assert_valid_rust_syntax(&code, "synthetic");
    assert!(code.contains("pub mod widget {"), "应生成 widget 模块");
    assert!(code.contains("pub fn create"), "应生成 create 函数");
    assert!(code.contains("pub fn greet"), "应生成 greet 方法");
    assert!(
        code.contains("pub async fn get_value"),
        "应生成 get_value 方法"
    );
    assert!(code.contains("pub fn destroy"), "应生成 destroy 方法");
    assert!(
        code.contains("serde::Serialize"),
        "Config struct 应有 Serialize"
    );
    assert!(!code.contains("Union("), "不应有 Union 类型");
}

#[test]
fn test_synthetic_union_null_generates_option() {
    let ts = r#"
export interface Result {
  getData: () => string | null
  destroy: () => void
}

export function createResult(): Result {
  return { getData: () => null, destroy: () => {} }
}

declare global {
  interface Window {
    __f: {
      createResult: () => Result
    }
    __result?: Result
  }
}
"#;

    let module = parse_synthetic(ts);
    let config = synthetic_config("__f");
    let code = generate_code(&module, &config);

    assert_valid_rust_syntax(&code, "union_null");
    assert!(
        code.contains("Option<String>"),
        "string | null 应生成 Option<String>，实际:\n{code}"
    );
}

#[test]
fn test_synthetic_string_or_promise_string() {
    let ts = r#"
export interface AsyncReader {
  read: () => string | Promise<string>
  destroy: () => void
}

export function createAsyncReader(): AsyncReader {
  return { read: () => "hello", destroy: () => {} }
}

declare global {
  interface Window {
    __f: {
      createAsyncReader: () => AsyncReader
    }
    __reader?: AsyncReader
  }
}
"#;

    let module = parse_synthetic(ts);
    let config = synthetic_config("__f");
    let code = generate_code(&module, &config);

    assert_valid_rust_syntax(&code, "string_or_promise_string");
    assert!(
        code.contains("-> String"),
        "string | Promise<string> 应生成 -> String，实际:\n{code}"
    );
}

#[test]
fn test_synthetic_custom_instance_var_mapping() {
    let ts = r#"
export interface Thing {
  doStuff: () => void
  destroy: () => void
}

export function createThing(): Thing {
  return { doStuff: () => {}, destroy: () => {} }
}

declare global {
  interface Window {
    __f: {
      createThing: () => Thing
    }
    __myThing?: Thing
  }
}
"#;

    let config = Config {
        entry: PathBuf::from("test.ts"),
        out_dir: PathBuf::from("/tmp"),
        global_factory_name: "__f".into(),
        instance_vars: Some(HashMap::from([("createThing".into(), "__myThing".into())])),
        ..Default::default()
    };

    let module = parse_synthetic(ts);
    let code = generate_code(&module, &config);

    assert_valid_rust_syntax(&code, "custom_instance_var");
    assert!(
        code.contains("__myThing"),
        "应使用自定义实例变量名 __myThing"
    );
}

#[test]
fn test_synthetic_no_params_create() {
    let ts = r#"
export interface Singleton {
  ping: () => void
  destroy: () => void
}

export function createSingleton(): Singleton {
  return { ping: () => {}, destroy: () => {} }
}

declare global {
  interface Window {
    __f: {
      createSingleton: () => Singleton
    }
    __singleton?: Singleton
  }
}
"#;

    let module = parse_synthetic(ts);
    let config = synthetic_config("__f");
    let code = generate_code(&module, &config);

    assert_valid_rust_syntax(&code, "no_params_create");
    assert!(
        code.contains("pub fn create<B: super::JsBridge>()"),
        "无参数 create 应生成空参数列表"
    );
}

#[test]
fn test_debug_parse_index_ts() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let ts_path = std::path::Path::new(&manifest_dir)
        .parent()
        .unwrap()
        .join("mobile/ts/src/index.ts");
    if ts_path.exists() {
        let ts = std::fs::read_to_string(&ts_path).unwrap();
        let module = parse_from_str(&ts, &ts_path).expect("parse_from_str failed");
        eprintln!("global_decl: {:?}", module.global_decl);
        if let Some(ref gd) = module.global_decl {
            eprintln!(
                "factory_methods: {:?}",
                gd.factory_methods
                    .iter()
                    .map(|f| &f.name)
                    .collect::<Vec<_>>()
            );
            eprintln!(
                "instance_vars: {:?}",
                gd.instance_vars.iter().map(|v| &v.name).collect::<Vec<_>>()
            );
        }
        assert!(module.global_decl.is_some(), "global_decl should be Some");
        let gd = module.global_decl.as_ref().unwrap();
        assert!(
            gd.factory_methods
                .iter()
                .any(|f| f.name == "__mt_note_mobile"),
            "__mt_note_mobile should be in factory_methods"
        );
    } else {
        eprintln!("File not found: {:?}", ts_path);
    }
}

#[test]
fn test_synthetic_primitive_params() {
    let ts = r#"
export interface Calculator {
  add: (a: number, b: number) => number
  destroy: () => void
}

export function createCalculator(id: string): Calculator {
  return { add: (a, b) => a + b, destroy: () => {} }
}

declare global {
  interface Window {
    __f: {
      createCalculator: (id: string) => Calculator
    }
    __calc?: Calculator
  }
}
"#;

    let module = parse_synthetic(ts);
    let config = synthetic_config("__f");
    let code = generate_code(&module, &config);

    assert_valid_rust_syntax(&code, "primitive_params");
    assert!(code.contains("id: &str"), "string 参数应为 &str");
    assert!(code.contains("-> f64"), "number 返回类型应为 f64");
    assert!(code.contains("a: f64"), "参数名应为 snake_case");
    assert!(code.contains("b: f64"), "参数名应为 snake_case");
}

#[test]
fn test_synthetic_rust_lint_expect_dead_code_with_reason() {
    let ts = r#"
export interface Bridge {
  /** @rust-lint expect(dead_code, reason = "由 Rust 侧调用") */
  destroy: () => void
}

export function createBridge(): Bridge {
  return { destroy: () => {} }
}

declare global {
  interface Window {
    __f: {
      createBridge: () => Bridge
    }
    __bridge?: Bridge
  }
}
"#;

    let module = parse_synthetic(ts);
    let config = synthetic_config("__f");
    let code = generate_code(&module, &config);

    assert_valid_rust_syntax(&code, "rust_lint_expect_dead_code_reason");
    assert!(
        code.contains("#[expect(dead_code, reason = \"由 Rust 侧调用\")]"),
        "应生成带原因的 expect dead_code 属性，实际:\n{code}"
    );
}

#[test]
fn test_synthetic_rust_lint_expect_dead_code_without_reason() {
    let ts = r#"
export interface Bridge {
  /** @rust-lint expect(dead_code) */
  destroy: () => void
}

export function createBridge(): Bridge {
  return { destroy: () => {} }
}

declare global {
  interface Window {
    __f: {
      createBridge: () => Bridge
    }
    __bridge?: Bridge
  }
}
"#;

    let module = parse_synthetic(ts);
    let config = synthetic_config("__f");
    let code = generate_code(&module, &config);

    assert_valid_rust_syntax(&code, "rust_lint_expect_dead_code_no_reason");
    assert!(
        code.contains("#[expect(dead_code)]"),
        "应生成 expect dead_code 属性，实际:\n{code}"
    );
}

#[test]
fn test_synthetic_rust_lint_on_interface() {
    let ts = r#"
/** @rust-lint allow(dead_code) */
export interface Store {
  getValue: () => string
  destroy: () => void
}

export function createStore(): Store {
  return { getValue: () => "v", destroy: () => {} }
}

declare global {
  interface Window {
    __f: {
      createStore: () => Store
    }
    __store?: Store
  }
}
"#;

    let module = parse_synthetic(ts);
    let config = synthetic_config("__f");
    let code = generate_code(&module, &config);

    assert_valid_rust_syntax(&code, "rust_lint_on_interface");
    assert!(
        code.contains("#[allow(dead_code)]"),
        "接口级 @rust-lint 应生成 allow 属性，实际:\n{code}"
    );
}

#[test]
fn test_synthetic_rust_lint_on_factory_function() {
    let ts = r#"
/** @rust-lint expect(dead_code, reason = "Rust 侧使用") */
export function createService(): Service {
  return { doWork: () => {}, destroy: () => {} }
}

export interface Service {
  doWork: () => void
  destroy: () => void
}

declare global {
  interface Window {
    __f: {
      createService: () => Service
    }
    __service?: Service
  }
}
"#;

    let module = parse_synthetic(ts);
    let config = synthetic_config("__f");
    let code = generate_code(&module, &config);

    assert_valid_rust_syntax(&code, "rust_lint_on_factory");
    assert!(
        code.contains("#[expect(dead_code, reason = \"Rust 侧使用\")]"),
        "工厂函数级 @rust-lint 应生成 expect 属性，实际:\n{code}"
    );
}

#[test]
fn test_synthetic_rust_lint_multiple_attrs() {
    let ts = r#"
export interface Widget {
  /**
   * @rust-lint expect(dead_code, reason = "未使用")
   * @rust-lint allow(clippy::too_many_arguments)
   */
  configure(a: string, b: number, c: boolean, d: string, e: number, f: boolean, g: string, h: number): void
  destroy: () => void
}

export function createWidget(): Widget {
  return { configure: (a, b, c, d, e, f, g, h) => {}, destroy: () => {} }
}

declare global {
  interface Window {
    __f: {
      createWidget: () => Widget
    }
    __widget?: Widget
  }
}
"#;

    let module = parse_synthetic(ts);
    let config = synthetic_config("__f");
    let code = generate_code(&module, &config);

    assert_valid_rust_syntax(&code, "rust_lint_multiple_attrs");
    assert!(
        code.contains("#[expect(dead_code, reason = \"未使用\")]"),
        "应包含 expect dead_code 属性，实际:\n{code}"
    );
    assert!(
        code.contains("#[allow(clippy::too_many_arguments)]"),
        "应包含 allow clippy 属性，实际:\n{code}"
    );
}

#[test]
fn test_synthetic_rust_lint_standalone_line() {
    let ts = r#"
export interface Handler {
  /**
   * @rust-lint expect(dead_code)
   */
  handle: () => void
  destroy: () => void
}

export function createHandler(): Handler {
  return { handle: () => {}, destroy: () => {} }
}

declare global {
  interface Window {
    __f: {
      createHandler: () => Handler
    }
    __handler?: Handler
  }
}
"#;

    let module = parse_synthetic(ts);
    let config = synthetic_config("__f");
    let code = generate_code(&module, &config);

    assert_valid_rust_syntax(&code, "rust_lint_standalone_line");
    assert!(
        code.contains("#[expect(dead_code)]"),
        "独立行 @rust-lint 应生成 expect 属性，实际:\n{code}"
    );
}

#[test]
fn test_synthetic_rust_lint_on_nested_path_method() {
    let ts = r#"
export interface Panel {
  render: {
    /** @rust-lint expect(dead_code, reason = "嵌套调用") */
    setScale(scale: number): void
    getScale(): number
  }
  destroy: () => void
}

export function createPanel(): Panel {
  return { render: { setScale: (s) => {}, getScale: () => 1.0 }, destroy: () => {} }
}

declare global {
  interface Window {
    __f: {
      createPanel: () => Panel
    }
    __panel?: Panel
  }
}
"#;

    let module = parse_synthetic(ts);
    let config = synthetic_config("__f");
    let code = generate_code(&module, &config);

    assert_valid_rust_syntax(&code, "rust_lint_nested_path");
    assert!(
        code.contains("#[expect(dead_code, reason = \"嵌套调用\")]"),
        "嵌套路径方法 @rust-lint 应生成 expect 属性，实际:\n{code}"
    );
    assert!(
        code.contains("render_set_scale"),
        "应生成嵌套路径 snake_case 方法名，实际:\n{code}"
    );
}

#[test]
fn test_synthetic_doc_comment_on_interface() {
    let ts = r#"
/** 数据存储配置 */
export interface StoreConfig {
  /** 存储路径 */
  path: string
}

export interface Store {
  getValue: () => string
  destroy: () => void
}

export function createStore(conf: StoreConfig): Store {
  return { getValue: () => "v", destroy: () => {} }
}

declare global {
  interface Window {
    __f: {
      createStore: (conf: StoreConfig) => Store
    }
    __store?: Store
  }
}
"#;
    let module = parse_synthetic(ts);
    let config = synthetic_config("__f");
    let code = generate_code(&module, &config);

    assert_valid_rust_syntax(&code, "doc_comment_interface");
    assert!(
        code.contains("/// 数据存储配置"),
        "应生成 struct doc comment，实际:\n{code}"
    );
}

#[test]
fn test_synthetic_doc_comment_on_method() {
    let ts = r#"
export interface Service {
  /** 获取当前状态 */
  getStatus: () => string
  destroy: () => void
}

export function createService(): Service {
  return { getStatus: () => "ok", destroy: () => {} }
}

declare global {
  interface Window {
    __f: {
      createService: () => Service
    }
    __service?: Service
  }
}
"#;
    let module = parse_synthetic(ts);
    let config = synthetic_config("__f");
    let code = generate_code(&module, &config);

    assert_valid_rust_syntax(&code, "doc_comment_method");
    assert!(
        code.contains("/// 获取当前状态"),
        "应生成方法 doc comment，实际:\n{code}"
    );
}

#[test]
fn test_synthetic_doc_comment_on_factory_function() {
    let ts = r#"
export interface Helper {
  help: () => void
  destroy: () => void
}

/** 创建辅助工具实例 */
export function createHelper(): Helper {
  return { help: () => {}, destroy: () => {} }
}

declare global {
  interface Window {
    __f: {
      createHelper: () => Helper
    }
    __helper?: Helper
  }
}
"#;
    let module = parse_synthetic(ts);
    let config = synthetic_config("__f");
    let code = generate_code(&module, &config);

    assert_valid_rust_syntax(&code, "doc_comment_factory");
    assert!(
        code.contains("/// 创建辅助工具实例"),
        "应生成 factory doc comment，实际:\n{code}"
    );
}

#[test]
fn test_synthetic_doc_comment_full_roundtrip() {
    let ts = r#"
/** 配置选项 */
export interface Options {
  /** 超时时间（毫秒） */
  timeout: number
}

/** 核心组件 */
export interface Component {
  /** 初始化组件 */
  init: (opts: Options) => void
  /** 获取当前状态 */
  getStatus: () => string
  destroy: () => void
}

/** 创建核心组件实例 */
export function createComponent(opts: Options): Component {
  return { init: () => {}, getStatus: () => "ok", destroy: () => {} }
}

declare global {
  interface Window {
    __f: {
      createComponent: (opts: Options) => Component
    }
    __component?: Component
  }
}
"#;
    let module = parse_synthetic(ts);
    let config = synthetic_config("__f");
    let code = generate_code(&module, &config);

    assert_valid_rust_syntax(&code, "doc_comment_roundtrip");
    assert!(
        code.contains("/// 配置选项"),
        "应包含 Options struct doc comment，实际:\n{code}"
    );
    assert!(
        code.contains("/// 创建核心组件实例"),
        "应包含 factory doc comment，实际:\n{code}"
    );
    assert!(
        code.contains("/// 初始化组件"),
        "应包含 init 方法 doc comment，实际:\n{code}"
    );
    assert!(
        code.contains("/// 获取当前状态"),
        "应包含 getStatus 方法 doc comment，实际:\n{code}"
    );
    assert!(
        code.contains("/// 超时时间（毫秒）"),
        "应包含 timeout 属性 doc comment，实际:\n{code}"
    );
}
