use std::collections::VecDeque;
use std::future::Future;
use std::io::Error;
use std::net::SocketAddr;
use std::num::NonZeroUsize;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::mpsc::{SendError, TryRecvError};
use std::sync::Mutex;
use std::task::{Context, Poll};
use std::thread;
use crate::network_tcp::session::{GET_UNARY_LIST_DELIMITER, GET_STREAM_LIST_DELIMITER, RawSession, OPEN_UNARY_DELIMITER, OPEN_STREAM_DELIMITER};
use crate::network_tcp::session::RawPacket;

struct OneShotMapFuture {
    key : Box<String>,
    map : Arc<Mutex<std::collections::HashMap<Box<String>, RawPacket>>>,
}

impl Future for OneShotMapFuture {
    type Output = RawPacket;
    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut lock_map = self.map.lock().unwrap();
        if let Some(obj) = lock_map.remove(&*self.key) {
            Poll::Ready(obj)
        } else {
            _cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}


pub(crate) struct RawClient {
    client_session : Arc<Mutex<RawSession>>,
    oneshot_map : Arc<Mutex<std::collections::HashMap<Box<String>, RawPacket>>>,
}

impl RawClient {
    pub(crate) async fn connect(sock_addr: SocketAddr,
                          session_timeout: Option<std::time::Duration>) -> Result<Self,std::io::Error> {
        match tokio::net::TcpStream::connect(sock_addr).await {
            Ok(mut connection) => {
                let peer_addr = connection.peer_addr().unwrap();
                let mut myself = Self {
                    client_session: Arc::new( Mutex::new(RawSession::new(connection,peer_addr,session_timeout))),
                    oneshot_map: Arc::new( Mutex::new(std::collections::HashMap::default())),
                };
                myself.session_control(session_timeout);
                Ok(myself)
            }
            Err(e) => {
                Err(e)
            }
        }
    }

    fn session_control(&mut self, session_timeout : Option<std::time::Duration>) {
        let arc_session = self.client_session.clone();
        let arc_session2 = self.client_session.clone();
        let arc_oneshot_map = self.oneshot_map.clone();
        println!("connect to server");

        thread::spawn(move || {
            let mut session_timeout_elapsed = 0;
            if let Some(value) = session_timeout {
                session_timeout_elapsed = value.as_secs();
            }
            let mut time_elapsed = std::time::Instant::now();
            let mut closed = false;
            let mut time_elapsed2 = std::time::Instant::now();
            let mut count = 0;
            loop {
                if closed {
                    break;
                }
                if session_timeout_elapsed > 0 {
                    if session_timeout_elapsed < time_elapsed.elapsed().as_secs() {
                        break;
                    }
                }

                let mut lock_session = arc_session.lock().unwrap();
                if !closed {
                    let mut lock_oneshot = arc_oneshot_map.lock().unwrap();
                    match lock_session.socket_read() {
                        Ok(read_packet) => {
                            time_elapsed = std::time::Instant::now();
                            for item in read_packet.iter() {
                                match item.delimiter {
                                    GET_UNARY_LIST_DELIMITER => {
                                        println!("GET_UNARY_LIST_DELIMITER");
                                        lock_oneshot.insert(Box::new(String::from("unary_id")),item.clone());
                                    }
                                    GET_STREAM_LIST_DELIMITER => {
                                        println!("GET_STREAM_LIST_DELIMITER");
                                        lock_oneshot.insert(Box::new(String::from("stream_id")),item.clone());
                                    }
                                    OPEN_UNARY_DELIMITER => {
                                        lock_oneshot.insert(item.get_str_id(),item.clone());
                                    }
                                    OPEN_STREAM_DELIMITER => {
                                        println!("count : {} , {}ms -> len : {}",count,time_elapsed2.elapsed().as_millis(),item.length);
                                        time_elapsed2 = std::time::Instant::now();
                                        count += 1;
                                    }
                                    _ => {
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            if e.kind() != std::io::ErrorKind::WouldBlock {
                                println!("{}",e);
                                break;
                            }
                        }
                    }
                }
            }
            println!("disconnect to server");
            arc_session.lock().unwrap().clear();
            drop(arc_session);
        });
    }

    pub(crate) async fn call_unary_list(&mut self) -> Result<Vec<u8>, std::io::Error> {
        let empty_id: [u8; 32] = [0; 32];
        let key_binary = vec![0;4];
        let test_future = OneShotMapFuture {
            key: Box::new("unary_id".to_string()),
            map: self.oneshot_map.clone(),
        };
        if let Err(e) = self.client_session.lock().unwrap().unsafe_socket_write(RawPacket::new(GET_UNARY_LIST_DELIMITER,
                                                    0,
                                                    1,
                                                    key_binary.len() as u32,
                                                    0,
                                                    empty_id,
                                                    key_binary)) {
            return Err(std::io::Error::new(std::io::ErrorKind::WouldBlock , "Channel Send Fail"));
        }
        Ok(test_future.await.data)
    }

    pub(crate) async fn call_stream_list(&mut self) -> Result<Vec<u8>, std::io::Error> {
        let empty_id: [u8; 32] = [0; 32];
        let key_binary = vec![0;4];
        let test_future = OneShotMapFuture {
            key: Box::new("stream_id".to_string()),
            map: self.oneshot_map.clone(),
        };

        if let Err(e) = self.client_session.lock().unwrap().unsafe_socket_write(RawPacket::new(GET_STREAM_LIST_DELIMITER,
                                                    0,
                                                    1,
                                                    key_binary.len() as u32,
                                                    0,
                                                    empty_id,
                                                    key_binary)) {
            return Err(std::io::Error::new(std::io::ErrorKind::WouldBlock , "Channel Send Fail"));
        }
        Ok(test_future.await.data)
    }

    pub(crate) async fn call_unary_data(&mut self,id : Box<String>) -> Result<Vec<u8>, std::io::Error> {
        let mut data_id: [u8; 32] = [0; 32];
        let key_binary = vec![0;4];
        let id_binary = id.as_bytes();
        data_id[0..id_binary.len()].copy_from_slice(id_binary);
        let unary_future = OneShotMapFuture {
            key: id,
            map: self.oneshot_map.clone(),
        };
        if let Err(e) = self.client_session.lock().unwrap().unsafe_socket_write(RawPacket::new(OPEN_UNARY_DELIMITER,
                                                    0,
                                                    1,
                                                    key_binary.len() as u32,
                                                    0,
                                                    data_id,
                                                    key_binary)) {
            return Err(std::io::Error::new(std::io::ErrorKind::WouldBlock , "Channel Send Fail"));
        }
        Ok(unary_future.await.data)
    }

    pub(crate) async fn call_stream_data(&mut self,id : Box<String>) -> Result<Vec<u8>, std::io::Error> {
        let mut data_id: [u8; 32] = [0; 32];
        let key_binary = vec![0;4];
        let id_binary = id.as_bytes();
        data_id[0..id_binary.len()].copy_from_slice(id_binary);

        if let Err(e) = self.client_session.lock().unwrap().unsafe_socket_write(RawPacket::new(OPEN_STREAM_DELIMITER,
                                                    0,
                                                    1,
                                                    key_binary.len() as u32,
                                                    0,
                                                    data_id,
                                                    key_binary)) {
            return Err(std::io::Error::new(std::io::ErrorKind::WouldBlock , "Channel Send Fail"));
        }
        Ok(vec![])
    }
}