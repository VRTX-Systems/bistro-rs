#[derive(Debug)]
pub enum RequestResponseError<T> {
    SendError(T),
    ResponseError,
}

#[derive(Clone)]
pub(crate) enum ServerMessage<T> {
    ClientDisconnected(usize),
    Data(usize, T)
}
