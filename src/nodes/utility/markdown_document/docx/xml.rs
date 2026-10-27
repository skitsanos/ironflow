use super::super::model::MAX_OUTPUT;
use crate::util::execution::ExecutionControl;
use anyhow::{Result, ensure};
use quick_xml::{
    Writer,
    events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event},
};

pub(super) struct Xml<'a> {
    writer: Writer<Vec<u8>>,
    execution: &'a ExecutionControl,
}

impl<'a> Xml<'a> {
    pub fn new(execution: &'a ExecutionControl) -> Result<Self> {
        let mut xml = Self {
            writer: Writer::new(Vec::new()),
            execution,
        };
        xml.event(Event::Decl(BytesDecl::new(
            "1.0",
            Some("UTF-8"),
            Some("yes"),
        )))?;
        Ok(xml)
    }
    pub fn start(&mut self, name: &str, attrs: &[(&str, &str)]) -> Result<()> {
        let mut tag = BytesStart::new(name);
        tag.extend_attributes(attrs.iter().copied());
        self.event(Event::Start(tag))
    }
    pub fn empty(&mut self, name: &str, attrs: &[(&str, &str)]) -> Result<()> {
        let mut tag = BytesStart::new(name);
        tag.extend_attributes(attrs.iter().copied());
        self.event(Event::Empty(tag))
    }
    pub fn end(&mut self, name: &str) -> Result<()> {
        self.event(Event::End(BytesEnd::new(name)))
    }
    pub fn text(&mut self, text: &str) -> Result<()> {
        self.event(Event::Text(BytesText::new(text)))
    }
    pub fn element(&mut self, name: &str, text: &str) -> Result<()> {
        self.start(name, &[])?;
        self.text(text)?;
        self.end(name)
    }
    pub fn finish(self) -> Vec<u8> {
        self.writer.into_inner()
    }
    fn event(&mut self, event: Event<'_>) -> Result<()> {
        self.execution.checkpoint()?;
        self.writer.write_event(event)?;
        ensure!(
            self.writer.get_ref().len() <= MAX_OUTPUT / 2,
            "document XML exceeds 16 MiB"
        );
        Ok(())
    }
}
