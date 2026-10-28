//! Font registry for `markdown_to_pdf`: the bundled Noto fonts, the optional
//! bundled CJK font (`cjk-fonts` feature) and fonts loaded once from
//! `IRONFLOW_FONT_DIR`. Registration happens once per process. The directory
//! is untrusted input, so its size is bounded before any file is read, and
//! every failure names the variable and the path.

use anyhow::{Result, anyhow, ensure};
use parley::{
    FontContext,
    fontique::{Blob, Collection, CollectionOptions, Script},
};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

pub(super) const FONT_DIR_ENV: &str = "IRONFLOW_FONT_DIR";
const MAX_FONT_FILES: usize = 64;
const MAX_FONT_FILE_BYTES: u64 = 64 * 1024 * 1024;
/// One representative code point per CJK script the generator distinguishes.
const CJK_PROBES: [char; 3] = ['中', 'あ', '한'];
const CJK_SCRIPTS: [[u8; 4]; 4] = [*b"Hani", *b"Hira", *b"Kana", *b"Hang"];

struct Registry {
    context: Mutex<FontContext>,
    cjk: bool,
}

pub(super) struct Fonts {
    pub context: FontContext,
    /// Whether any registered font covers CJK text, so the shaper can fail
    /// with a pointer to the feature or the font directory before layout.
    pub cjk_available: bool,
}

pub(super) fn context() -> Result<Fonts> {
    static FONTS: OnceLock<Result<Registry, String>> = OnceLock::new();
    let registry = FONTS.get_or_init(|| initialize().map_err(|error| format!("{error:#}")));
    match registry {
        Ok(registry) => Ok(Fonts {
            context: registry
                .context
                .lock()
                .map_err(|_| anyhow!("font registry lock poisoned"))?
                .clone(),
            cjk_available: registry.cjk,
        }),
        Err(error) => anyhow::bail!("font registry initialization failed: {error}"),
    }
}

fn initialize() -> Result<Registry> {
    let mut context = FontContext {
        collection: Collection::new(CollectionOptions {
            shared: true,
            system_fonts: false,
        }),
        ..Default::default()
    };
    let bundled: &[&'static [u8]] = &[
        include_bytes!("../../../../../assets/fonts/NotoSerif.ttf"),
        include_bytes!("../../../../../assets/fonts/NotoSerif-Italic.ttf"),
        include_bytes!("../../../../../assets/fonts/NotoSansMono.ttf"),
        include_bytes!("../../../../../assets/fonts/NotoSansArabic.ttf"),
        include_bytes!("../../../../../assets/fonts/NotoSansHebrew.ttf"),
    ];
    for &source in bundled {
        register_bundled(&mut context, source)?;
    }
    #[cfg(feature = "cjk-fonts")]
    register_bundled(
        &mut context,
        include_bytes!("../../../../../assets/fonts/NotoSansCJK.otf"),
    )?;
    let mut cjk = cfg!(feature = "cjk-fonts");
    if let Some(directory) = std::env::var_os(FONT_DIR_ENV) {
        cjk |= load_directory(&mut context, Path::new(&directory))?;
    }
    Ok(Registry {
        context: Mutex::new(context),
        cjk,
    })
}

fn register_bundled(context: &mut FontContext, source: &'static [u8]) -> Result<()> {
    ensure!(
        !context
            .collection
            .register_fonts(Blob::new(Arc::new(source)), None)
            .is_empty(),
        "cannot register bundled font"
    );
    Ok(())
}

/// Register every `.ttf`, `.otf` and `.ttc` file in `directory` and make the
/// resulting families fallbacks for the CJK scripts, so a font with any family
/// name serves CJK runs. Returns whether one of them covers CJK text.
fn load_directory(context: &mut FontContext, directory: &Path) -> Result<bool> {
    let unreadable = |error: std::io::Error| {
        anyhow!(
            "{FONT_DIR_ENV}: cannot read {}: {error}",
            directory.display()
        )
    };
    let mut files: Vec<PathBuf> = Vec::new();
    for entry in std::fs::read_dir(directory).map_err(unreadable)? {
        let path = entry.map_err(unreadable)?.path();
        let is_font = path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                matches!(
                    extension.to_ascii_lowercase().as_str(),
                    "ttf" | "otf" | "ttc"
                )
            });
        if is_font {
            files.push(path);
        }
    }
    files.sort();
    ensure!(
        files.len() <= MAX_FONT_FILES,
        "{FONT_DIR_ENV}: {} holds {} font files; at most {MAX_FONT_FILES} are loaded",
        directory.display(),
        files.len()
    );
    let mut families = Vec::new();
    let mut cjk = false;
    for path in files {
        let size = std::fs::metadata(&path)
            .map_err(|error| anyhow!("{FONT_DIR_ENV}: cannot read {}: {error}", path.display()))?
            .len();
        ensure!(
            size <= MAX_FONT_FILE_BYTES,
            "{FONT_DIR_ENV}: {} is {size} bytes; fonts above {MAX_FONT_FILE_BYTES} bytes are not loaded",
            path.display()
        );
        let bytes =
            Arc::new(std::fs::read(&path).map_err(|error| {
                anyhow!("{FONT_DIR_ENV}: cannot read {}: {error}", path.display())
            })?);
        let registered = context
            .collection
            .register_fonts(Blob::new(bytes.clone()), None);
        ensure!(
            !registered.is_empty(),
            "{FONT_DIR_ENV}: {} is not a usable font file",
            path.display()
        );
        for (family, fonts) in registered {
            families.push(family);
            cjk |= fonts.iter().any(|font| {
                font.charmap_index()
                    .charmap(bytes.as_slice())
                    .is_some_and(|charmap| {
                        CJK_PROBES
                            .iter()
                            .any(|probe| charmap.map(*probe).is_some_and(|glyph| glyph != 0))
                    })
            });
        }
    }
    for script in CJK_SCRIPTS {
        context
            .collection
            .append_fallbacks(Script::from_bytes(script), families.iter().copied());
    }
    Ok(cjk)
}
