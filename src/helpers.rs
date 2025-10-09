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

#[derive(Debug, Clone)]
pub struct Breadcrumb {
    pub name: String,
    pub path: String,
}

pub fn make_remote_breadcrumbs(current_path: &str) -> Vec<Breadcrumb> {
    let mut crumbs = Vec::new();
    let mut path_accum = String::new();

    for part in current_path.trim_matches('/').split('/') {
        if part.is_empty() {
            continue;
        }

        path_accum.push('/');
        path_accum.push_str(part);

        crumbs.push(Breadcrumb {
            name: part.to_string(),
            path: path_accum.clone(),
        });
    }

    crumbs
}
