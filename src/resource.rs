use axum::response::Response;
use kata::{ParseError, Template};
use std::{collections::HashMap, string::FromUtf8Error};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to load resource file: {0}")]
    InvalidUtf8(#[from] FromUtf8Error),

    #[error("failed to parse template: {0}")]
    BadTemplate(#[from] ParseError),

    #[error("template '{0}' not found")]
    TemplateNotFound(&'static str),
}

pub enum MimeType {
    Text,
    Html,
    Css,
    Js,
    Png,
    Jpg,
    OctetStream,
}

impl MimeType {
    fn from_extension(extension: &str) -> MimeType {
        match extension {
            ".txt" => MimeType::Text,
            ".html" => MimeType::Html,
            ".js" => MimeType::Js,
            ".jpg" | ".jpeg" => MimeType::Jpg,
            ".png" => MimeType::Png,
            ".css" => MimeType::Css,
            _ => MimeType::OctetStream,
        }
    }

    fn is_binary(&self) -> bool {
        match self {
            MimeType::Text => false,
            MimeType::Html => false,
            MimeType::Css => false,
            MimeType::Js => false,
            MimeType::Png => true,
            MimeType::Jpg => true,
            MimeType::OctetStream => true,
        }
    }
}

impl core::fmt::Display for MimeType {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            MimeType::Text => "text/plain",
            MimeType::Html => "text/html",
            MimeType::Css => "text/css",
            MimeType::Js => "application/javascript",
            MimeType::Png => "image/png",
            MimeType::Jpg => "image/jpg",
            MimeType::OctetStream => "application/octet-stream",
        })
    }
}

enum ResourceData {
    Binary(Vec<u8>),
    String(String),
}

pub struct ResourceFile {
    mime_type: String,
    data: ResourceData,
}

impl ResourceFile {
    pub fn new(mime_type: MimeType, data: Vec<u8>) -> Result<Self, Error> {
        let resource_data = if mime_type.is_binary() {
            ResourceData::Binary(data)
        } else {
            ResourceData::String(String::from_utf8(data)?)
        };

        Ok(Self {
            mime_type: mime_type.to_string(),
            data: resource_data,
        })
    }

    pub fn as_bytes(&self) -> &[u8] {
        match &self.data {
            ResourceData::Binary(bin) => bin,
            ResourceData::String(str) => str.as_bytes(),
        }
    }

    fn to_binary(&self) -> Vec<u8> {
        match &self.data {
            ResourceData::Binary(bin) => bin.to_owned(),
            ResourceData::String(str) => str.as_bytes().to_owned(),
        }
    }
}

impl core::fmt::Display for ResourceFile {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match &self.data {
            ResourceData::Binary(bin) => {
                for chunk in bin.utf8_chunks() {
                    f.write_str(chunk.valid())?;
                    for b in chunk.invalid() {
                        write!(f, "\\x{b:02x}")?;
                    }
                }
                Ok(())
            }
            ResourceData::String(str) => f.write_str(str),
        }
    }
}

impl From<&ResourceFile> for Response {
    fn from(value: &ResourceFile) -> Self {
        Response::builder()
            .header("Content-Type", &value.mime_type)
            .body(value.to_binary().into())
            .unwrap()
    }
}

pub struct ResourceManager {
    resource_files: HashMap<String, ResourceFile>,
    template_cache: HashMap<String, Template>,
}

impl ResourceManager {
    pub fn new() -> Self {
        Self {
            resource_files: HashMap::new(),
            template_cache: HashMap::new(),
        }
    }

    pub fn register_resource(&mut self, name: &str, data: &[u8]) -> Result<(), Error> {
        self.resource_files.insert(
            name.to_string(),
            ResourceFile::new(Self::infer_mime(name), data.to_owned())?,
        );
        Ok(())
    }

    pub fn register_template(&mut self, name: &str, data: &[u8]) -> Result<(), Error> {
        self.register_resource(name, data)?;
        if let Some(res) = self.find_resource(name) {
            let res_str = res.to_string();
            let template = Template::compile(&res_str)?;
            self.template_cache.insert(name.to_owned(), template);
        }
        Ok(())
    }

    pub fn find_resource(&self, name: &str) -> Option<&ResourceFile> {
        self.resource_files.get(name)
    }

    pub fn find_template(&self, name: &'static str) -> Result<&Template, Error> {
        self.template_cache
            .get(name)
            .ok_or(Error::TemplateNotFound(name))
    }

    fn infer_mime(name: &str) -> MimeType {
        let extension_sep_idx = name.rfind('.');
        match extension_sep_idx {
            Some(extension_sep_idx) => {
                let ext = &name[extension_sep_idx..];
                MimeType::from_extension(ext)
            }
            None => MimeType::OctetStream,
        }
    }
}
