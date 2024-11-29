use std::collections::HashSet;
use async_std::channel;
use crate::errors::BartenderErrors;
use crate::pubsub::async_std::*;
use crate::reqres::common::*;

pub struct AsyncRequestToken<'svr, Request,Response>
where
    Request: Send + Sync + Clone,
    Response: Send + Sync + Clone,
{
    value: Request,
    id: usize,
    server: &'svr mut AsyncServer<Request, Response>,
}

impl <'svr, Request, Response> AsyncRequestToken<'svr, Request, Response>
where
    Request: Send + Sync + Clone,
    Response: Send + Sync + Clone,
{
    pub async fn submit(self, value: Response) -> Result<(), BartenderErrors> {
        self.server.response_channel.publish(self.id.into(), value).await?;
        Ok(())
    }
}

pub struct AsyncServer<Request, Response>
where
    Request: Send + Sync + Clone,
    Response: Send + Sync + Clone,
{
    response_channel: AsyncPublisher<Response>,
    request_channel: channel::Receiver<ServerMessage<Request>>,
    request_prototype: channel::Sender<ServerMessage<Request>>,
    connected_clients: HashSet<usize>
}

impl<Request, Response> AsyncServer<Request, Response>
where
    Request: Send + Sync + Clone,
    Response: Send + Sync + Clone,
{
    pub fn new() -> Self {
        let (tx, rx) = channel::unbounded();
        AsyncServer {
            response_channel: AsyncPublisher::new(),
            request_channel: rx,
            request_prototype: tx,
            connected_clients: HashSet::new()
        }
    }

    pub fn new_client(&mut self) -> Result<AsyncClient<Request, Response>, BartenderErrors> {
        let id = {
            let mut id = 0;
            while self.connected_clients.contains(&id) {
                id += 1;
            }
            id
        };
        self.connected_clients.insert(id);
        let response = self.response_channel.subscribe(&[id.into()])?;
        let client = AsyncClient {
            request: self.request_prototype.clone(),
            response,
            id,
        };
        Ok(client)
    }

    pub async fn recv(&mut self) -> Result<AsyncRequestToken<Request, Response>, BartenderErrors> {
        loop {
            match self.request_channel.recv().await {
                Ok(ServerMessage::ClientDisconnected(id)) => {
                    self.connected_clients.remove(&id);
                    if self.connected_clients.is_empty() {
                        return Err(BartenderErrors::Disconnected)
                    }
                }
                Ok(ServerMessage::Data(id, value)) => {
                    let out = AsyncRequestToken {
                        value,
                        id,
                        server: self,
                    };
                    return Ok(out);
                }
                Err(_) => { return Err(BartenderErrors::Disconnected) }
            }
        }


    }
}

pub struct AsyncClient<Request, Response>
where
    Request: Send + Sync + Clone,
    Response: Send + Sync + Clone,
{
    request: channel::Sender<ServerMessage<Request>>,
    response: AsyncSubscriber<Response>,
    id: usize,
}



impl<Request, Response> AsyncClient<Request, Response>
where
    Request: Send + Sync + Clone,
    Response: Send + Sync + Clone,
{
    pub async fn call(&mut self, req: Request) -> Result<Response, RequestResponseError<Request>> {
        match self.request.send(ServerMessage::Data(self.id, req)).await{
            Ok(_) => {}
            Err(channel::SendError(msg)) => {
                if let ServerMessage::Data(_, data) = msg {
                    return Err(RequestResponseError::SendError(data));
                } else {
                    unreachable!();
                }
            }
        }

        self.response
            .recv()
            .await
            .map_err(|_| RequestResponseError::ResponseError)
    }
}
impl<Request, Response> Drop for AsyncClient<Request, Response>
where
    Request: Send + Sync + Clone,
    Response: Send + Sync + Clone,
{
    fn drop(&mut self) {
        self.request.send_blocking(ServerMessage::ClientDisconnected(self.id)).unwrap()
    }
}


#[cfg(test)]
mod tests {
    use super::*;
    use smol;
    async fn client_thread(mut client: AsyncClient<usize, usize>) -> usize {
        let n_requests: usize= 10;
        for i in 0..n_requests {
            let response = client.call(i).await.unwrap();
            println!("response: {:?}", response);
            assert_eq!(response, i);
        }
        n_requests
    }

    async fn server_thread(mut server: AsyncServer<usize, usize>)-> usize{
        let mut count = 0;
        while let Ok(request) = server.recv().await {
            let value = request.value.clone();
            let result = request.submit(value).await.unwrap();
            count += 1;
        }
        count
    }
    #[test]
    fn api() {
        use smol::future;
        let mut server = AsyncServer::new();
        let client1 = server.new_client().unwrap();
        let client2 = server.new_client().unwrap();

        let server_future = smol::spawn(async  move {server_thread(server).await});

        let test_task = smol::spawn(async {
            let client_1_future = client_thread(client1);
            let client_2_future = client_thread(client2);
            let (count_1, count_2) = future::zip(client_1_future, client_2_future).await;
            assert!(count_1 > 0);
            assert!(count_2 > 0);
            count_1 + count_2
        });

        let join_task = smol::block_on(
            async {
                let client_total = test_task.await;
                let server_count = server_future.await;
                assert!(client_total > 0);
                assert!(server_count > 0);
                assert_eq!(client_total, server_count);
            }
        );
    }
}