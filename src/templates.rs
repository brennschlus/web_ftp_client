use askama::Template;

use crate::routes::FileInfo;
#[derive(Template)]
#[template(path = "index.html")]
pub struct IndexTemplate {}

#[derive(Template)]
#[template(path = "files_table.html")]
pub struct FilesTableTemplate {
    pub files: Vec<FileInfo>,
}
