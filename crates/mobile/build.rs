fn main() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set");

    let stylance_config = stylance_cli::Config::load(manifest_dir.clone().into())
        .expect("Failed to load stylance config");
    stylance_cli::run(&stylance_config).expect("Stylance failed");

    generate_locale_registry(&manifest_dir, &out_dir);

    let ts_dir = format!("{manifest_dir}/ts");
    let ts_entry = format!("{ts_dir}/src/index.ts");
    let ts_path = std::path::Path::new(&ts_entry);

    if ts_path.exists() {
        let ts_dist = format!("{ts_dir}/dist");
        let ts_bundle = format!("{ts_dist}/mt-note-mobile-ts.js");

        generate_ts_icons(&manifest_dir, &ts_dir);

        let installed = ensure_deps_installed(&ts_dir);
        // 依赖刚变动时必须重新 bundle：node_modules 内容变了，旧 dist 与新依赖
        // 混搭会产出「主 bundle 与 worker 版本不一致」的坏包
        let need_build = installed || need_rebuild(&ts_dir, &ts_bundle);
        if need_build {
            pnpm_run("build", &ts_dir, &[]);
        }

        js_bridge_bindgen::generate(&js_bridge_bindgen::Config {
            entry: ts_path.to_path_buf(),
            out_dir: std::path::PathBuf::from(&out_dir),
            global_factory_name: "__mt_note_mobile".into(),
            ..Default::default()
        })
        .expect("js-bridge-bindgen codegen failed");

        copy_assets(&ts_dir, &ts_dist);
    }

    declare_rerun_deps(&manifest_dir, &ts_dir);
}

fn declare_rerun_deps(manifest_dir: &str, ts_dir: &str) {
    println!("cargo:rerun-if-changed={manifest_dir}/assets/styles/*.scss");
    println!("cargo:rerun-if-changed={manifest_dir}/src/frontend/**/*.scss");
    println!("cargo:rerun-if-changed={ts_dir}/src");
    // 依赖清单：变化会改变 node_modules 安装结果（ensure_deps_installed 指纹比对）
    println!("cargo:rerun-if-changed={ts_dir}/package.json");
    println!("cargo:rerun-if-changed={ts_dir}/pnpm-lock.yaml");
    println!("cargo:rerun-if-changed={ts_dir}/pnpm-workspace.yaml");
    // 构建与门禁配置：变化影响产物或代码通过标准，必须重跑 build.rs（need_rebuild 比对 mtime）
    println!("cargo:rerun-if-changed={ts_dir}/tsconfig.json");
    println!("cargo:rerun-if-changed={ts_dir}/vite.config.ts");
    println!("cargo:rerun-if-changed={ts_dir}/.oxlintrc.json");
    println!("cargo:rerun-if-changed={ts_dir}/.prettierignore");
}

/// 扫描 locales/*.ftl 生成 OUT_DIR/locales.gen.rs 语言注册表：语言集合完全
/// 数据驱动，新增语言只需放入 `<langid>.ftl`（BCP-47 完整标签，如
/// `zh-Hans.ftl`；icons.gen.ts 同款「目录即真源」）。同时强制全部文件键集
/// 一致——dioxus-i18n 的字符串键无编译期检查（缺 id 运行期才暴露），此处是
/// 协作一致性的唯一机器强制点，每次构建（含 lint 宿主编译）生效。
/// 唯一硬性要求：`en.ftl` 必须存在（en 是回退基准语言，见
/// models::data::language 模块文档）。
fn generate_locale_registry(manifest_dir: &str, out_dir: &str) {
    let locales_dir = format!("{manifest_dir}/locales");

    let mut files: Vec<String> = std::fs::read_dir(&locales_dir)
        .unwrap_or_else(|e| unreachable!("读取 {locales_dir} 失败: {e}"))
        .flatten()
        .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("ftl"))
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    files.sort();
    println!("cargo:rerun-if-changed={locales_dir}");
    // 目录声明只覆盖增删/改名（目录 mtime），文件内容修改不动目录时间——
    // 必须逐文件声明，否则改译文后 build.rs 不重跑，生成的 strings-gen.xml
    // （Android 侧译文）停留在旧内容。
    for name in &files {
        println!("cargo:rerun-if-changed={locales_dir}/{name}");
    }
    assert!(
        files.iter().any(|f| f == "en.ftl"),
        "locales/en.ftl 缺失：en 是回退基准语言（语言协商无匹配与缺键的兜底、\
         Android 默认资源 values/），必须存在"
    );

    // 极简行解析：非缩进、非注释的 `id = value` 行；缩进行是 Fluent 多行续值，天然跳过
    let parse_msgs = |name: &str| -> Vec<(String, String)> {
        let path = format!("{locales_dir}/{name}");
        let src = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| unreachable!("读取 {path} 失败: {e}"));
        src.lines()
            .filter(|line| !line.starts_with('#') && !line.starts_with([' ', '\t']))
            .filter(|line| !line.trim().is_empty())
            .filter_map(|line| line.split_once('='))
            .map(|(id, value)| (id.trim().to_string(), value.trim().to_string()))
            .collect()
    };

    let reference = parse_msgs(&files[0]);
    assert!(
        !reference.is_empty(),
        "locales/{} 未解析到任何消息 id，解析疑似失效",
        files[0]
    );
    for name in &files[1..] {
        let ids: Vec<String> = parse_msgs(name).into_iter().map(|(id, _)| id).collect();
        let missing: Vec<&String> = reference.iter().map(|(id, _)| id).filter(|id| !ids.contains(id)).collect();
        let extra: Vec<&String> = ids.iter().filter(|id| !reference.iter().any(|(r, _)| r == *id)).collect();
        assert!(
            missing.is_empty() && extra.is_empty(),
            "翻译键集不一致：{name} 相对 {} 缺 {missing:?} 多 {extra:?}",
            files[0]
        );
    }

    // Android 字符串资源：仅 `native-` 前缀键进入 strings-gen.xml（避免全量
    // 生成臃肿）。前缀标记「需进原生平台资源」（不绑平台名，未来其他原生端
    // 同样适用），键本身全平台共用——TS/Rust 取词不分前缀，避免同一译文
    // 重复命名多个键。
    // en 是 Android 资源系统的默认语言（设备未匹配语言时回退 values/），
    // 其余语言进按 BCP-47 标签映射的资源目录。内容不变不重写，避免 mtime 抖动。
    let escape_xml = |s: &str| -> String {
        s.replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('\'', "&apos;")
            .replace('"', "&quot;")
    };
    let mut res_dirs: Vec<String> = Vec::new();
    for name in &files {
        let code = name
            .strip_suffix(".ftl")
            .unwrap_or_else(|| unreachable!("locales 文件名 {name} 缺 .ftl 后缀"));
        let res_dir = if code == "en" {
            "values".to_string()
        } else {
            android_res_dir(code)
        };
        res_dirs.push(res_dir.clone());
        let mut body = String::from(
            "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
             <!-- 由 mobile/build.rs 从 locales/ 的 native- 前缀键自动生成，请勿手动编辑。 -->\n\
             <resources>\n",
        );
        for (id, value) in parse_msgs(name) {
            let Some(res_name) = id.strip_prefix("native-") else {
                continue;
            };
            body.push_str(&format!(
                "    <string name=\"{}\">{}</string>\n",
                res_name.replace('-', "_"),
                escape_xml(&value)
            ));
        }
        body.push_str("</resources>\n");
        let gen_dir = format!("{manifest_dir}/android/res/{res_dir}");
        std::fs::create_dir_all(&gen_dir).expect("创建 android res 目录失败");
        let gen_path = format!("{gen_dir}/strings-gen.xml");
        let changed = std::fs::read_to_string(&gen_path)
            .map(|old| old != body)
            .unwrap_or(true);
        if changed {
            std::fs::write(&gen_path, body).expect("写 strings-gen.xml 失败");
        }
    }
    clean_stale_generated_strings(manifest_dir, &res_dirs);

    let mut body = String::from(
        "// 由 mobile/build.rs 从 locales/ 目录自动生成，请勿手动编辑。\n\
         // 新增语言：放入 <langid>.ftl 即自动注册，键集一致性由 build.rs 强制。\n\
         // 类型与宏用全限定路径，避免与宿主模块的导入冲突。\n\
         pub static LOCALE_FILES: &[(dioxus_i18n::unic_langid::LanguageIdentifier, &str)] = &[\n",
    );
    for name in &files {
        let id = name.strip_suffix(".ftl").unwrap_or_else(|| {
            unreachable!("locales 文件名 {name} 缺 .ftl 后缀");
        });
        body.push_str(&format!(
            "    (dioxus_i18n::unic_langid::langid!(\"{id}\"), include_str!(\"{locales_dir}/{name}\")),\n"
        ));
    }
    body.push_str("];\n");

    let gen_path = format!("{out_dir}/locales.gen.rs");
    std::fs::write(&gen_path, body).expect("写 locales.gen.rs 失败");
}

/// BCP-47 标签 → Android 资源目录名：带文字子标签（4 字母，如 zh-Hans）用
/// `b+` 形式；仅语言+地区（如 pt-BR、es-419）用 `-r` 形式；文字+地区组合
/// 同样走 `b+` 形式。en 不经此函数（作默认语言直接进 values/）。
fn android_res_dir(code: &str) -> String {
    let mut segments = code.split('-');
    let lang = segments
        .next()
        .unwrap_or_else(|| unreachable!("locales 语言标签 {code} 为空"));
    assert!(!lang.is_empty(), "locales 语言标签 {code} 缺主语言子标签");
    match segments.next() {
        Some(script) if script.len() == 4 => match segments.next() {
            Some(region) if !region.is_empty() => format!("values-b+{lang}+{script}+r{region}"),
            _ => format!("values-b+{lang}+{script}"),
        },
        Some(region) if !region.is_empty() => format!("values-{lang}-r{region}"),
        _ => format!("values-{lang}"),
    }
}

/// 清理过期的 strings-gen.xml：翻译文件改名/删除后旧资源目录会与新目录并存，
/// Android 资源系统按同名资源报 duplicate。只删生成物本身，不碰目录内其他
/// 文件（如手写 strings.xml）。
fn clean_stale_generated_strings(manifest_dir: &str, current_dirs: &[String]) {
    let res_root = format!("{manifest_dir}/android/res");
    let entries = match std::fs::read_dir(&res_root) {
        Ok(entries) => entries,
        Err(e) => unreachable!("读取 {res_root} 失败: {e}"),
    };
    for entry in entries.flatten() {
        let dir_name = match entry.file_name().into_string() {
            Ok(name) => name,
            Err(e) => unreachable!("res 目录名非 UTF-8: {e:?}"),
        };
        if !dir_name.starts_with("values") || current_dirs.contains(&dir_name) {
            continue;
        }
        let stale = entry.path().join("strings-gen.xml");
        if stale.is_file() {
            std::fs::remove_file(&stale)
                .unwrap_or_else(|e| unreachable!("清理过期生成物 {} 失败: {e}", stale.display()));
        }
    }
}

/// 从 ui 包 icons 模块提取图标 path，生成 ts/src/icons.gen.ts——TS 侧图标的
/// 单一真源在 Rust（ui/src/components/icons/mod.rs 的 icons! 宏），TS 侧经
/// icons.ts 的 iconSvg() 包壳使用，禁止再手写内联 <svg>。
/// 内容不变时不重写，避免 mtime 抖动经 need_rebuild 触发无谓的 pnpm 重构建。
fn generate_ts_icons(manifest_dir: &str, ts_dir: &str) {
    let icons_rs = format!("{manifest_dir}/../ui/src/components/icons/mod.rs");
    println!("cargo:rerun-if-changed={icons_rs}");
    let src = std::fs::read_to_string(&icons_rs)
        .unwrap_or_else(|e| unreachable!("读取 {icons_rs} 失败: {e}"));

    // 逐行匹配 `Variant => "path",`（icons! 块内每图标一行，注释行跳过）
    let mut pairs: Vec<(&str, &str)> = src
        .lines()
        .map(str::trim)
        .filter(|line| !line.starts_with("//"))
        .filter_map(|line| {
            let (variant, path) = line.split_once("=>")?;
            let path = path.trim().strip_prefix('"')?;
            let path = path.strip_suffix("\",")?;
            Some((variant.trim(), path))
        })
        .collect();
    pairs.sort_by(|a, b| a.0.cmp(b.0));

    let mut body =
        String::from("// 由 mobile/build.rs 从 ui 包 icons 模块自动生成，请勿手动编辑。\n");
    body.push_str("export const ICON_PATHS = {\n");
    for (variant, path) in &pairs {
        body.push_str(&format!("  {variant}: \"{path}\",\n"));
    }
    body.push_str("} as const;\nexport type IconName = keyof typeof ICON_PATHS;\n");

    let gen_path = format!("{ts_dir}/src/icons.gen.ts");
    let changed = std::fs::read_to_string(&gen_path)
        .map(|old| old != body)
        .unwrap_or(true);
    if changed {
        std::fs::write(&gen_path, body).expect("write icons.gen.ts failed");
    }
}

fn pnpm_run(subcmd: &str, cwd: &str, envs: &[(&str, &str)]) {
    let mut cmd = if cfg!(windows) {
        let mut c = std::process::Command::new("cmd");
        c.args(["/C", &format!("pnpm {subcmd}")]);
        c
    } else {
        let mut c = std::process::Command::new("pnpm");
        c.args([subcmd]);
        c
    };
    cmd.current_dir(cwd);
    for (k, v) in envs {
        cmd.env(k, v);
    }
    let exit = cmd
        .status()
        .unwrap_or_else(|e| unreachable!("pnpm {subcmd}: failed to execute: {e}"));
    assert!(exit.success(), "pnpm {subcmd} failed with {exit}");
}

/// 判断是否需要重新 pnpm build：dist 不存在，或构建相关配置、ts/src 下任意
/// .ts 比 dist 新。配置文件（tsconfig/vite/lint）变化会影响产物内容或代码
/// 通过标准，与源码一视同仁。
fn need_rebuild(ts_dir: &str, dist: &str) -> bool {
    if !std::path::Path::new(dist).exists() {
        return true;
    }
    let dist_time = match std::fs::metadata(dist).and_then(|m| m.modified()) {
        Ok(t) => t,
        Err(e) => unreachable!("读取 dist mtime 失败: {e}"),
    };
    const BUILD_CONFIGS: [&str; 4] = [
        "tsconfig.json",
        "vite.config.ts",
        ".oxlintrc.json",
        ".prettierignore",
    ];
    for name in BUILD_CONFIGS {
        if let Ok(t) = std::fs::metadata(format!("{ts_dir}/{name}")).and_then(|m| m.modified())
            && t > dist_time
        {
            return true;
        }
    }
    let mut stack = vec![std::path::PathBuf::from(format!("{ts_dir}/src"))];
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(e) => unreachable!("读取目录 {dir:?} 失败: {e}"),
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|e| e.to_str()) == Some("ts")
                && let Ok(t) = path.metadata().and_then(|m| m.modified())
                && t > dist_time
            {
                return true;
            }
        }
    }
    false
}

/// 依赖安装对账：package.json / pnpm-lock.yaml / pnpm-workspace.yaml 的内容
/// 指纹存于 node_modules/.install-stamp，指纹变化或 node_modules 缺失时重跑
/// `pnpm install` 并返回 true（调用方须接着重新 bundle）。
/// 背景：cargo 的 rerun-if-changed 只会让 build.rs 重跑，而此前 install 仅在
/// node_modules 缺失时执行——升依赖版本既不重装也不重新 bundle，打出旧产物。
fn ensure_deps_installed(ts_dir: &str) -> bool {
    let node_modules = format!("{ts_dir}/node_modules");
    let stamp_path = format!("{node_modules}/.install-stamp");
    let mut fingerprint = String::new();
    for name in ["package.json", "pnpm-lock.yaml", "pnpm-workspace.yaml"] {
        fingerprint.push_str(&format!("# ==== {name} ====\n"));
        let missing = String::from("<missing>\n");
        fingerprint.push_str(&std::fs::read_to_string(format!("{ts_dir}/{name}")).unwrap_or(missing));
        fingerprint.push('\n');
    }
    let stamp_matches = std::fs::read_to_string(&stamp_path)
        .map(|old| old == fingerprint)
        .unwrap_or(false);
    if std::path::Path::new(&node_modules).exists() && stamp_matches {
        return false;
    }
    pnpm_run("install", ts_dir, &[]);
    std::fs::write(&stamp_path, fingerprint).expect("写 install 指纹失败");
    true
}

/// 复制不能打包进 JS bundle 的静态资源从 node_modules 到 ts/dist/。
fn copy_assets(ts_dir: &str, ts_dist: &str) {
    let node_modules = format!("{ts_dir}/node_modules");

    copy_file(
        &format!("{node_modules}/katex/dist/katex.min.css"),
        &format!("{ts_dist}/katex/katex.min.css"),
    );
    copy_dir(
        &format!("{node_modules}/katex/dist/fonts"),
        &format!("{ts_dist}/katex/fonts"),
    );
    copy_file(
        &format!("{node_modules}/pdfjs-dist/build/pdf.worker.min.mjs"),
        &format!("{ts_dist}/pdfjs/pdf.worker.min.mjs"),
    );
    copy_dir(
        &format!("{node_modules}/pdfjs-dist/cmaps"),
        &format!("{ts_dist}/pdfjs/cmaps"),
    );
}

fn copy_file(src: &str, dst: &str) {
    let src_path = std::path::Path::new(src);
    if src_path.exists() {
        if let Some(parent) = std::path::Path::new(dst).parent() {
            std::fs::create_dir_all(parent).expect("create dir failed");
        }
        std::fs::copy(src, dst).expect("copy file failed");
        println!("cargo:rerun-if-changed={src}");
    }
}

fn copy_dir(src: &str, dst: &str) {
    if std::path::Path::new(src).exists() {
        copy_dir_recursive(std::path::Path::new(src), std::path::Path::new(dst))
            .expect("copy dir failed");
        println!("cargo:rerun-if-changed={src}");
    }
}

fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}
