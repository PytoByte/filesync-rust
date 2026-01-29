use reqwest_dav::{Auth, Client, ClientBuilder, Depth, list_cmd::{ListEntity, ListFile}};
use tokio::{fs::{self, File}, io::{AsyncWriteExt}};
use std::{cmp::Ordering, fs::Metadata, path::Path};
use chrono::{DateTime, Utc};
use crate::{state::{Message, SyncState}};
use iced::futures::{SinkExt, channel::mpsc};

pub async fn run_sync(mut output: mpsc::Sender<Message>, host: String, login: String, password: String, pairs: Vec<(String, String)>) {
    let client = ClientBuilder::new()
        .set_host(host)
        .set_auth(Auth::Basic(login, password))
        .build().unwrap();

    if !check_connection(&client).await {
        let _ = output.send(Message::ShowError(String::from("Can't open connection"))).await;
        let _ = output.send(Message::StopSynchronize).await;
        return;
    }

    synchronize_files(&client, &mut output, &pairs).await;
    
    let _ = output.send(Message::StopSynchronize).await;
}

pub async fn check_sync(mut output: mpsc::Sender<Message>, host: String, login: String, password: String, pairs: Vec<(String, String)>) {
    let client = ClientBuilder::new()
        .set_host(host)
        .set_auth(Auth::Basic(login, password))
        .build().unwrap();

    if !check_connection(&client).await {
        let _ = output.send(Message::ShowError(String::from("Can't open connection"))).await;
        let _ = output.send(Message::StopSynchronizeCheck).await;
        return;
    }

    synchronize_files_check(&client, &mut output, &pairs).await;
    
    let _ = output.send(Message::StopSynchronizeCheck).await;
}

async fn check_connection(client: &Client) -> bool {
    client.list("/", Depth::Number(0)).await.is_ok()
}

async fn is_local_file_exist(filepath: &str) -> bool {
    Path::new(filepath).exists()
}

async fn is_remote_file_exist(client: &Client, filepath: &str) -> bool {
    client.list_raw(filepath, Depth::Number(0)).await.unwrap().status() != 404
}

async fn get_remote_file_info(client: &Client, filepath: &str) -> Option<ListFile> {
    let response = client.list(filepath, Depth::Number(0)).await;

    match response {
        Ok(listvec) => {
            if let Some(ListEntity::File(listfile)) = listvec.first() {
                Some(listfile.clone())
            } else {
                None
            }
        },
        _ => None
    }
}

async fn get_local_file_info(filepath: &str) -> Option<Metadata> {
    fs::metadata(Path::new(filepath)).await.ok()
}

async fn download_file(client: &Client, local_path: &str, server_path: &str) -> Option<()> {
    let result = client.get(server_path).await.ok();

    if let Some(response) = result {
        if response.status().is_success() {
            let bytes = response.bytes().await.ok();
            let file = File::create(local_path).await.ok();
            if let Some(mut ready_file) = file {
                if let Some(ready_bytes) = bytes {
                    match ready_file.write_all(&ready_bytes).await {
                        Ok(..) => {
                            return Some(());
                        },
                        Err(..) => {
                            return None;
                        }
                    }
                }
            }
        }
    }
    
    return Some(());
}

async fn ensure_remote_directories(client: &Client, server_path: &str) -> bool {
    let dir_path = Path::new(server_path)
        .parent()
        .and_then(|p| p.to_str())
        .unwrap_or("");

    if dir_path.is_empty() || dir_path == "/" {
        return true;
    }

    let parts: Vec<&str> = dir_path.trim_start_matches('/').split('/').collect();
    let mut current_path = String::from("/");

    for part in parts {
        if part.is_empty() {
            continue;
        }
        current_path.push_str(part);
        current_path.push('/');
        
        match client.mkcol_raw(&current_path).await {
            Ok(response) => {
                if response.status() != 405 {
                    return false;
                }
            },
            Err(..) => {
                return false;
            }
        }
    }

    true
}

async fn upload_file(client: &Client, local_path: &str, server_path: &str) -> Option<()> {
    if !ensure_remote_directories(client, server_path).await {
        return None;
    }

    let result_content = tokio::fs::read(local_path).await.ok();
    if let Some(content) = result_content {
        client.put(server_path, content).await.unwrap();
        return Some(());
    }
    
    return None;
}

async fn is_download_possible(local_path: &str) -> bool {
    Path::new(local_path).parent().unwrap().exists()
}

async fn compare_modified_time(client: &Client, local_path: &str, server_path: &str) -> Option<std::cmp::Ordering> {
    if let Some(metadata) = get_local_file_info(local_path).await {
        if let Some(filelist) = get_remote_file_info(client, server_path).await {
            let metadata_dt: DateTime<Utc> = metadata.modified().unwrap().into();
            return Some(metadata_dt.cmp(&filelist.last_modified));
        }
    }
    
    return None;
}

async fn synchronize_file(client: &Client, output: &mut mpsc::Sender<Message>, local_path: &str, server_path: &str) -> bool {
    if is_local_file_exist(local_path).await && is_remote_file_exist(client, server_path).await {
        if let Some(ordering) = compare_modified_time(client, local_path, server_path).await {
            match ordering {
                Ordering::Greater => {
                    if upload_file(client, local_path, server_path).await.is_some() {
                        return output.send(Message::UpdatePairSyncState(local_path.to_owned(), SyncState::Synchronized)).await.is_ok();
                    }
                },
                Ordering::Less => {
                    if download_file(client, local_path, server_path).await.is_some() {
                        return output.send(Message::UpdatePairSyncState(local_path.to_owned(), SyncState::Synchronized)).await.is_ok();
                    }
                },
                _ => {
                    return output.send(Message::UpdatePairSyncState(local_path.to_owned(), SyncState::Synchronized)).await.is_ok();
                }
            }

        }
    } else if is_local_file_exist(local_path).await && !is_remote_file_exist(client, server_path).await {
        if upload_file(client, local_path, server_path).await.is_some() {
            return output.send(Message::UpdatePairSyncState(local_path.to_owned(), SyncState::Synchronized)).await.is_ok();
        }
    } else if !is_local_file_exist(local_path).await && is_remote_file_exist(client, server_path).await {
        if is_download_possible(local_path).await {
            if download_file(client, local_path, server_path).await.is_some() {
                return output.send(Message::UpdatePairSyncState(local_path.to_owned(), SyncState::Synchronized)).await.is_ok();
            }
        }
    }
    return output.send(Message::UpdatePairSyncState(local_path.to_owned(), SyncState::CantSynchronize)).await.is_err();
}

async fn synchronize_files(client: &Client, output: &mut mpsc::Sender<Message>, pairs: &Vec<(String, String)>) -> bool {
    for (k, v) in pairs.iter() {
        if !synchronize_file(client, output, k, v).await {
            return false;
        }
    }
    return true;
}

async fn synchronize_file_check(client: &Client, output: &mut mpsc::Sender<Message>, local_path: &str, server_path: &str) -> bool {
    if is_local_file_exist(local_path).await && is_remote_file_exist(client, server_path).await {
        if let Some(ordering) = compare_modified_time(client, local_path, server_path).await {
            match ordering {
                Ordering::Greater => {
                    return output.send(Message::UpdatePairSyncState(local_path.to_owned(), SyncState::UnsynchronizedServer)).await.is_ok();
                },
                Ordering::Less => {
                    return output.send(Message::UpdatePairSyncState(local_path.to_owned(), SyncState::UnsynchronizedDevice)).await.is_ok();
                },
                _ => {
                    return output.send(Message::UpdatePairSyncState(local_path.to_owned(), SyncState::Synchronized)).await.is_ok();
                }
            }

        }
    } else if is_local_file_exist(local_path).await && !is_remote_file_exist(client, server_path).await {
        return output.send(Message::UpdatePairSyncState(local_path.to_owned(), SyncState::UnsynchronizedServer)).await.is_ok();
    } else if !is_local_file_exist(local_path).await && is_remote_file_exist(client, server_path).await {
        if is_download_possible(local_path).await {
            return output.send(Message::UpdatePairSyncState(local_path.to_owned(), SyncState::UnsynchronizedDevice)).await.is_ok();
        }
    }
    return output.send(Message::UpdatePairSyncState(local_path.to_owned(), SyncState::CantSynchronize)).await.is_err();
}

async fn synchronize_files_check(client: &Client, output: &mut mpsc::Sender<Message>, pairs: &Vec<(String, String)>) -> bool {
    for (key, value) in pairs.iter() {
        if !synchronize_file_check(client, output, key, value).await {
            return false;
        }
    }
    return true;
}