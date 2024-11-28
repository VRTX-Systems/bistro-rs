use std::collections::HashSet;
use crate::errors::BartenderErrors;
use crate::pubsub::sync::{Publisher, Subscriber};
use std::sync::mpsc;
use crate::reqres::common::*;


impl<T> From<mpsc::SendError<T>> for RequestResponseError<T> {
    fn from(value: mpsc::SendError<T>) -> Self {
        Self::SendError(value.0)
    }
}

pub struct RequestToken<'svr, Request,Response>
where
    Request: Send + Sync + Clone,
    Response: Send + Sync + Clone,
{
    value: Request,
    id: usize,
    server: &'svr mut Server<Request, Response>,
}

impl <'svr, Request, Response> RequestToken<'svr, Request, Response>
where
    Request: Send + Sync + Clone,
    Response: Send + Sync + Clone,
{
    pub fn submit(mut self, value: Response) -> Result<(), BartenderErrors> {
        self.server.response_channel.publish(self.id.into(), value)?;
        Ok(())
    }
}

struct Server<Request, Response>
where
    Request: Send + Sync + Clone,
    Response: Send + Sync + Clone,
{
    response_channel: Publisher<Response>,
    request_channel: mpsc::Receiver<ServerMessage<Request>>,
    request_prototype: mpsc::Sender<ServerMessage<Request>>,
    connected_clients: HashSet<usize>
}

impl<Request, Response> Server<Request, Response>
where
    Request: Send + Sync + Clone,
    Response: Send + Sync + Clone,
{
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Server {
            response_channel: Publisher::new(),
            request_channel: rx,
            request_prototype: tx,
            connected_clients: HashSet::new()
        }
    }

    pub fn new_client(&mut self) -> Result<Client<Request, Response>, BartenderErrors> {
        let id = {
            let mut id = 0;
            while self.connected_clients.contains(&id) {
                id += 1;
            }
            id
        };
        self.connected_clients.insert(id);
        let response = self.response_channel.subscribe(&[id.into()])?;
        let client = Client {
            request: self.request_prototype.clone(),
            response,
            id,
        };
        Ok(client)
    }

    pub fn recv(&mut self) -> Result<RequestToken<Request, Response>, BartenderErrors> {
        loop {
            match self.request_channel.recv() {
                Ok(ServerMessage::ClientDisconnected(id)) => {
                    self.connected_clients.remove(&id);
                    if self.connected_clients.is_empty() {
                        return Err(BartenderErrors::Disconnected)
                    }
                }
                Ok(ServerMessage::Data(id, value)) => {
                    let out = RequestToken {
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

struct Client<Request, Response>
where
    Request: Send + Sync + Clone,
    Response: Send + Sync + Clone,
{
    request: mpsc::Sender<ServerMessage<Request>>,
    response: Subscriber<Response>,
    id: usize,
}



impl<Request, Response> Client<Request, Response>
where
    Request: Send + Sync + Clone,
    Response: Send + Sync + Clone,
{
    fn call(&mut self, req: Request) -> Result<Response, RequestResponseError<Request>> {
        match self.request.send(ServerMessage::Data(self.id, req)){
            Ok(_) => {}
            Err(mpsc::SendError(msg)) => {
                if let ServerMessage::Data(_, data) = msg {
                    return Err(RequestResponseError::SendError(data));
                } else {
                    unreachable!();
                }
            }
        }

        self.response
            .recv()
            .map_err(|_| RequestResponseError::ResponseError)
    }
}
impl<Request, Response> Drop for Client<Request, Response>
where
    Request: Send + Sync + Clone,
    Response: Send + Sync + Clone,
{
    fn drop(&mut self) {
        self.request.send(ServerMessage::ClientDisconnected(self.id)).unwrap()
    }
}


#[cfg(test)]
mod tests {
    use crate::reqres::sync::{Client, Server};

    fn client_thread(mut client: Client<usize, usize>) -> usize{
        let n_requests:usize= 10;
        for i in 0..n_requests {
            let response = client.call(i).unwrap();
            assert_eq!(response, i);
        }
        n_requests
    }

    fn server_thread(mut server: Server<usize, usize>)-> usize{
        let mut count = 0;
        while let Ok(request) = server.recv() {
            let value = request.value.clone();
            let result = request.submit(value);
            count += 1;
        }
        count
    }
    #[test]
    fn api() {

        let mut server = Server::new();
        let client1 = server.new_client().unwrap();
        let client2 = server.new_client().unwrap();
        let server_thread = std::thread::spawn(move || {server_thread(server)});
        let client1_thread= std::thread::spawn(
            move || {client_thread(client1)}
        );
      let client2_thread= std::thread::spawn(move || {client_thread(client2)});

        let count_1 = client1_thread.join().unwrap();
       let count_2 = client2_thread.join().unwrap();
        let server_count = server_thread.join().unwrap();
        assert!(count_1 > 0);
         assert!(count_2 > 0);
        assert!(server_count  > 0 );
        assert_eq!(count_1 + count_2, server_count);


    }
}