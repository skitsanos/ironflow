use super::{super::assets::Picture, Xml};
use anyhow::Result;

pub(super) fn write(
    xml: &mut Xml<'_>,
    picture: &Picture,
    relationship: usize,
    instance: usize,
    alt: &str,
) -> Result<()> {
    let width = (picture.width * 12700.0).round().to_string();
    let height = (picture.height * 12700.0).round().to_string();
    xml.start("w:p", &[])?;
    xml.start("w:r", &[])?;
    xml.start("w:drawing", &[])?;
    xml.start(
        "wp:inline",
        &[
            (
                "xmlns:wp",
                "http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing",
            ),
            ("distT", "0"),
            ("distB", "0"),
            ("distL", "0"),
            ("distR", "0"),
        ],
    )?;
    xml.empty("wp:extent", &[("cx", &width), ("cy", &height)])?;
    xml.empty(
        "wp:docPr",
        &[
            ("id", &instance.to_string()),
            ("name", &format!("Image {instance}")),
            ("descr", alt),
        ],
    )?;
    xml.start(
        "a:graphic",
        &[(
            "xmlns:a",
            "http://schemas.openxmlformats.org/drawingml/2006/main",
        )],
    )?;
    xml.start(
        "a:graphicData",
        &[(
            "uri",
            "http://schemas.openxmlformats.org/drawingml/2006/picture",
        )],
    )?;
    xml.start(
        "pic:pic",
        &[(
            "xmlns:pic",
            "http://schemas.openxmlformats.org/drawingml/2006/picture",
        )],
    )?;
    xml.start("pic:nvPicPr", &[])?;
    xml.empty(
        "pic:cNvPr",
        &[
            ("id", "0"),
            ("name", &format!("image{relationship}.png")),
            ("descr", alt),
        ],
    )?;
    xml.empty("pic:cNvPicPr", &[])?;
    xml.end("pic:nvPicPr")?;
    xml.start("pic:blipFill", &[])?;
    xml.empty("a:blip", &[("r:embed", &format!("image{relationship}"))])?;
    xml.start("a:stretch", &[])?;
    xml.empty("a:fillRect", &[])?;
    xml.end("a:stretch")?;
    xml.end("pic:blipFill")?;
    xml.start("pic:spPr", &[])?;
    xml.start("a:xfrm", &[])?;
    xml.empty("a:off", &[("x", "0"), ("y", "0")])?;
    xml.empty("a:ext", &[("cx", &width), ("cy", &height)])?;
    xml.end("a:xfrm")?;
    xml.start("a:prstGeom", &[("prst", "rect")])?;
    xml.empty("a:avLst", &[])?;
    xml.end("a:prstGeom")?;
    xml.end("pic:spPr")?;
    xml.end("pic:pic")?;
    xml.end("a:graphicData")?;
    xml.end("a:graphic")?;
    xml.end("wp:inline")?;
    xml.end("w:drawing")?;
    xml.end("w:r")?;
    xml.end("w:p")
}
