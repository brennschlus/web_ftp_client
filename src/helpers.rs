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
