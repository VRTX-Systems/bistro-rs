use async_std;
use crate::errors::BartenderErrors;
use crate::pubsub::topics::{TopicMask, MAX_TOPICS, Topic};

impl From<async_std::channel::RecvError> for BartenderErrors  {
    fn from(_: async_std::channel::RecvError) -> Self {
        BartenderErrors::Disconnected
    }
}
impl<T> From<async_std::channel::SendError<T>> for BartenderErrors  {
    fn from(value: async_std::channel::SendError<T>) -> Self {
        BartenderErrors::Disconnected
    }
}

pub struct AsyncPublisher<T>
where T: Send +Clone
{
    senders: Vec<(TopicMask, async_std::channel::Sender<T>)>,
    topics: [Option<Topic>; MAX_TOPICS]
}


impl<T> AsyncPublisher<T>
where T: Send + Clone
{
    pub fn new() -> AsyncPublisher<T> {
        AsyncPublisher {
                senders: Vec::new(),
                topics: [None; MAX_TOPICS]
            }

    }

    fn total_topics(&self) -> usize {
        self.topics.iter().filter(|&t| t.is_some()).count()
    }

    fn get_mask(&self, topic: Topic) -> Option<TopicMask> {
        let mut mask: u64 = 0;
        for (bit, t) in self.topics.iter().enumerate() {
            if Some(topic) == *t {
                mask += 1 << bit;
            }
        }
        if mask == 0 {
            None
        }
        else {
            Some(TopicMask::new(mask))
        }
    }

    pub async fn publish(&self, topic: Topic, message: T) -> Result<(), BartenderErrors> {
        if let Some(topic_mask) = self.get_mask(topic) {
            for (mask, sub) in &self.senders {
                if mask.contains(&topic_mask) {
                    sub.send(message.clone()).await?;
                }
            }
        }
        Ok(())
    }

    pub fn subscribe(&mut self, topics: &[Topic]) -> Result<AsyncSubscriber<T>, BartenderErrors> {
        let mut this_mask: TopicMask = TopicMask::new(0);
        for &topic in topics {
            let mask = {
                if let Some(mask) = self.get_mask(topic) {
                    mask
                } else {
                    self.new_mask(topic)?
                }
            };
            this_mask = this_mask | mask;
        }
        let (tx,rx) = async_std::channel::unbounded();

        self.senders.push((this_mask, tx));
        Ok(AsyncSubscriber { rx })
    }

    fn new_mask(&mut self, topic: Topic) -> Result<TopicMask, BartenderErrors> {
        let mut index = None;
        for (i, t) in self.topics.iter().enumerate() {
            match t {
                &None => {if index.is_none() {
                    index = Some(i)
                }}
                &Some(t) => { if t == topic { return Err(BartenderErrors::TopicExists) } }
            }
        }
        if let Some(index) = index {
            self.topics[index] = Some(topic);
            Ok(TopicMask::new(1 << index))
        } else {
            Err(BartenderErrors::TopicsExhausted)
        }
    }
}


pub struct AsyncSubscriber<T> {
    rx: async_std::channel::Receiver<T>
}

impl <T> AsyncSubscriber<T> {
    pub async fn recv(&self) -> Result<T, BartenderErrors> {
        self.rx.recv().await.map_err(|_| BartenderErrors::Disconnected)
    }
    pub fn try_recv(&self) -> Result<Option<T>, BartenderErrors> {
        match self.rx.try_recv() {
            Ok(value) => Ok(Some(value)),
            Err(async_std::channel::TryRecvError::Empty) => Ok(None),
            Err(async_std::channel::TryRecvError::Closed) => Err(BartenderErrors::Disconnected),
        }
    }
}

#[cfg(test)]
mod test {
    use std::ops::Div;
    use super::*;

   async fn count_messages<T>(sub: AsyncSubscriber<T>) -> usize {
       let mut count = 0;
       while let Ok(message) = sub.recv().await {
            count += 1;
       }
        count
    }

    #[test]
    fn api() {

        let mut publisher = AsyncPublisher::new();
        let foo_sub = publisher.subscribe(&["foo".into()]).unwrap();
        let bar_sub = publisher.subscribe(&["bar".into()]).unwrap();
        let foobar_sub = publisher.subscribe(&["foo".into(), "bar".into()]).unwrap();

        assert_eq!(publisher.total_topics(), 2);
        assert_eq!(publisher.senders.len(), 3);

        let foo_task = smol::spawn(async move {count_messages(foo_sub).await});
        let bar_task = smol::spawn(async move{ count_messages(bar_sub).await });
        let foobar_task = smol::spawn(async move { count_messages(foobar_sub).await });

        let pub_task = smol::spawn(async move {
            for i in 1..1000usize {
                if i % 3 == 0 {
                    publisher.publish("foo".into(), i).await.unwrap();
                } else if i % 5 == 0 {
                    publisher.publish("bar".into(), i).await.unwrap();
                }
            }
        });

        let result = async move {
            let foo_count = foo_task.await;
            let bar_count = bar_task.await;
            let foobar_count = foobar_task.await;
            assert_eq!(foo_count, 1000usize.div(3));
            assert_ne!(bar_count, 1000usize.div(5));
            assert_eq!(foo_count + bar_count, foobar_count)
        };

        smol::block_on(pub_task);
        smol::block_on(result);
    }
}