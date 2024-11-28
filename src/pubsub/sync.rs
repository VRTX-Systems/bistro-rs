use std::sync;
use crate::errors::*;
use crate::pubsub::topics::*;

impl<T> From<sync::mpsc::SendError<T>> for BartenderErrors {
    fn from(_: sync::mpsc::SendError<T>) -> Self {
        BartenderErrors::Disconnected
    }
}
impl From<sync::mpsc::RecvError> for BartenderErrors {
    fn from(_: sync::mpsc::RecvError) -> Self {
        BartenderErrors::Disconnected
    }
}


pub struct Publisher<T>
where T: Send + Clone
{
    senders: Vec<(TopicMask, sync::mpsc::Sender<T>)>,
    topics: [Option<Topic>; MAX_TOPICS]
}


impl<T> Publisher<T>
where T: Send + Clone
{
    pub fn new() -> Publisher<T> {
        Publisher {
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


    pub fn publish(&self, topic: Topic, message: T) -> Result<(), BartenderErrors> {
        if let Some(topic_mask) = self.get_mask(topic) {
            for (mask, sub) in &self.senders {
                if mask.contains(&topic_mask) {
                    sub.send(message.clone())?;
                }
            }
        }
        Ok(())
    }

    pub fn subscribe(&mut self, topics: &[Topic]) -> Result<Subscriber<T>, BartenderErrors> {
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
        let (tx,rx) = sync::mpsc::channel();

        self.senders.push((this_mask, tx));
        Ok(Subscriber { rx })
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


pub struct Subscriber<T> {
    rx: sync::mpsc::Receiver<T>
}

impl <T> Subscriber<T> {
pub    fn recv(&self) -> Result<T, BartenderErrors> {
        self.rx.recv().map_err(|_| BartenderErrors::Disconnected)
    }
pub    fn try_recv(&self) -> Result<Option<T>, BartenderErrors> {
        match self.rx.try_recv() {
            Ok(t) => Ok(Some(t)),
            Err(sync::mpsc::TryRecvError::Empty) => Ok(None),
            Err(sync::mpsc::TryRecvError::Disconnected) => Err(BartenderErrors::Disconnected),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::ops::Div;
    use super::*;

    fn count_messages<T>(sub: Subscriber<T>) -> usize{
        let mut count = 0;
        loop{
            match sub.recv() {
                Ok(_) => {count+=1;}
                Err(BartenderErrors::Disconnected) => {
                    return count;
                }
                Err(e) => {panic!("{:?}", e)} ,
            }
        }
    }

    #[test]
    fn str_api() {

        let (foo, bar, foobar) = {
            let mut publisher = Publisher::new();
            let foo_sub = publisher.subscribe(&["foo".into()]).unwrap();
            let bar_sub = publisher.subscribe(&["bar".into()]).unwrap();
            let foobar_sub = publisher.subscribe(&["foo".into(), "bar".into()]).unwrap();

            assert_eq!(publisher.total_topics(), 2);
            assert_eq!(publisher.senders.len(), 3);


            let foo_thread = std::thread::spawn(move || { count_messages(foo_sub) });
            let bar_thread = std::thread::spawn(move || { count_messages(bar_sub) });
            let foobar_thread = std::thread::spawn(move || { count_messages(foobar_sub) });


            for i in 1..1000usize {
                if i % 3 == 0 {
                    publisher.publish("foo".into(), i).unwrap();
                } else if i % 5 == 0 {
                    publisher.publish("bar".into(), i).unwrap();
                }
            }
            (foo_thread, bar_thread, foobar_thread)
        };
        let foo_count = foo.join().unwrap();
        let bar_count = bar.join().unwrap();
        let foobar_count = foobar.join().unwrap();

        assert_eq!(foo_count, 1000usize.div(3));
        assert_ne!(bar_count, 1000usize.div(5));
        assert_eq!(foo_count + bar_count, foobar_count)

    }

    #[test]
    fn usize_api() {

        let (foo, bar, foobar) = {
            let mut publisher = Publisher::new();
            let foo_sub = publisher.subscribe(&[0usize.into()]).unwrap();
            let bar_sub = publisher.subscribe(&[1usize.into()]).unwrap();
            let foobar_sub = publisher.subscribe(&[0usize.into(), 1usize.into()]).unwrap();

            assert_eq!(publisher.total_topics(), 2);
            assert_eq!(publisher.senders.len(), 3);


            let foo_thread = std::thread::spawn(move || { count_messages(foo_sub) });
            let bar_thread = std::thread::spawn(move || { count_messages(bar_sub) });
            let foobar_thread = std::thread::spawn(move || { count_messages(foobar_sub) });


            for i in 1..1000usize {
                if i % 3 == 0 {
                    publisher.publish(0usize.into(), i).unwrap();
                } else if i % 5 == 0 {
                    publisher.publish(1usize.into(), i).unwrap();
                }
            }
            (foo_thread, bar_thread, foobar_thread)
        };
        let foo_count = foo.join().unwrap();
        let bar_count = bar.join().unwrap();
        let foobar_count = foobar.join().unwrap();

        assert_eq!(foo_count, 1000usize.div(3));
        assert_ne!(bar_count, 1000usize.div(5));
        assert_eq!(foo_count + bar_count, foobar_count)

    }
}
