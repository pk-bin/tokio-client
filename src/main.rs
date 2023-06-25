use std::net::SocketAddr;
use std::str::FromStr;
use crate::network_tcp::client::RawClient;

mod network_tcp;

#[tokio::main]
async fn main() {
    match RawClient::connect(SocketAddr::from_str("127.0.0.1:7777").unwrap(), None).await {
        Ok(mut raw_client) => {
            match raw_client.call_unary_list().await {
                Ok(obj) => {
                    for item in std::str::from_utf8(obj.as_slice()).unwrap().split(',') {
                        println!("call_unary_list item : {}",item);
                    }
                }
                Err(_) => {}
            }

            match raw_client.call_stream_list().await {
                Ok(obj) => {
                    for item in std::str::from_utf8(obj.as_slice()).unwrap().split(',') {
                        println!("call_stream_list item : {}",item);
                    }
                }
                Err(_) => {}
            }

            match raw_client.call_stream_data(Box::new(String::from("echo2"))).await {
                Ok(result) => {
                }
                Err(_) => {}
            }

            loop {
                std::thread::sleep(std::time::Duration::from_secs(3));
            }
        }
        Err(_) => {

        }
    }
}
