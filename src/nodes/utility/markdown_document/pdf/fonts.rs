use anyhow::{Result, ensure};
use parley::{
    FontContext,
    fontique::{Blob, Collection, CollectionOptions},
};
use std::sync::{Arc, Mutex, OnceLock};

pub(super) fn context() -> Result<FontContext> {
    static FONTS: OnceLock<Result<Mutex<FontContext>, String>> = OnceLock::new();
    let fonts = FONTS.get_or_init(|| {
        initialize()
            .map(Mutex::new)
            .map_err(|error| error.to_string())
    });
    match fonts {
        Ok(fonts) => Ok(fonts
            .lock()
            .map_err(|_| anyhow::anyhow!("font registry lock poisoned"))?
            .clone()),
        Err(error) => anyhow::bail!("font registry initialization failed: {error}"),
    }
}

fn initialize() -> Result<FontContext> {
    let mut context = FontContext {
        collection: Collection::new(CollectionOptions {
            shared: true,
            system_fonts: false,
        }),
        ..Default::default()
    };
    let sources: &[&'static [u8]] = &[
        include_bytes!("../../../../../assets/fonts/NotoSerif.ttf"),
        include_bytes!("../../../../../assets/fonts/NotoSerif-Italic.ttf"),
        include_bytes!("../../../../../assets/fonts/NotoSansMono.ttf"),
        include_bytes!("../../../../../assets/fonts/NotoSansArabic.ttf"),
        include_bytes!("../../../../../assets/fonts/NotoSansHebrew.ttf"),
        include_bytes!("../../../../../assets/fonts/NotoSansCJK.otf"),
    ];
    for &source in sources {
        ensure!(
            !context
                .collection
                .register_fonts(Blob::new(Arc::new(source)), None)
                .is_empty(),
            "cannot register bundled font"
        );
    }
    Ok(context)
}
