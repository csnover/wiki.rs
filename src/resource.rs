use std::collections::HashMap;

use kata::Template;
use axum::response::Response;

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
    pub fn new(mime_type: MimeType, data: Vec<u8>) -> Self {
        let resource_data = if mime_type.is_binary() {
            ResourceData::Binary(data)
        } else {
            ResourceData::String(String::from_utf8(data).expect("Failed to load resource file"))
        };

        Self {
            mime_type: mime_type.to_string(),
            data: resource_data,
        }
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
        f.write_str(match &self.data {
            ResourceData::Binary(bin) => {
                str::from_utf8(bin).expect("Failed conversion to_string")
            }
            ResourceData::String(str) => str
        })
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

    pub fn register_resource(&mut self, name: &str, data: &[u8]) {
        self.resource_files.insert(
            name.to_string(),
            ResourceFile::new(Self::infer_mime(name), data.to_owned()),
        );
    }

    pub fn register_template(&mut self, name: &str, data: &[u8]) {
        self.register_resource(name, data);
        if let Some(res) = self.find_resource(name) {
            let res_str = res.to_string();
            let template = Template::compile(&res_str).expect("Failed to compile template");
            self.template_cache.insert(name.to_owned(), template);
        }
    }

    pub fn find_resource(&self, name: &str) -> Option<&ResourceFile> {
        self.resource_files.get(name)
    }

    pub fn find_template(&self, name: &str) -> Option<&Template> {
        self.template_cache.get(name)
    }

    fn infer_mime(name: &str) -> MimeType {
        let extension_sep_idx = name.rfind(".");
        match extension_sep_idx {
            Some(extension_sep_idx) => {
                let ext = &name[extension_sep_idx..];
                MimeType::from_extension(ext)
            }
            None => MimeType::OctetStream,
        }
    }
}
