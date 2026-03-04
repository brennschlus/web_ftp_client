use suppaftp::tokio::AsyncFtpStream;

pub async fn is_connected(state: &mut Option<AsyncFtpStream>) -> bool {
    if let Some(ftp) = state {
        match ftp.noop().await {
            Ok(_) => true,
            Err(_) => {
                *state = None;
                false
            }
        }
    } else {
        false
    }
}

// pub fn render_file_table(files: Vec<String>) -> Html<String> {
//     let files = files
//         .into_iter()
//         .map(|item| File::from_str(&item))
//         .into_iter()
//         .flatten()
//         .map(|item| FileInfo::from(&item))
//         .collect::<Vec<_>>();
//     Html(FilesTableTemplate { files }.render().unwrap())
// }
