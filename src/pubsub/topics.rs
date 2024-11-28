#[derive(Eq, PartialEq, Debug, Clone, Copy)]
pub enum Topic{
    StrTopic(&'static str),
    IntTopic(usize)
}

impl From<&'static str> for Topic{
    fn from(topic: &'static str) -> Self{
        Topic::StrTopic(topic)
    }
}
impl From<usize> for Topic{
    fn from(topic: usize) -> Self{
        Topic::IntTopic(topic)
    }
}


pub(crate) struct TopicMask {
    mask: u64
}
impl TopicMask {
pub(crate)    fn contains(&self, mask: &TopicMask) -> bool {
        self.mask & mask.mask != 0
    }
pub(crate)    fn is_empty(&self) -> bool {
        self.mask == 0
    }
    pub(crate) fn new(mask: u64) -> Self {
        TopicMask{mask}
    }
}
impl std::ops::BitOr for TopicMask {
    type Output = TopicMask;
    fn bitor(self, rhs: Self) -> Self::Output {
        TopicMask{mask: self.mask | rhs.mask}
    }
}

pub const MAX_TOPICS: usize = 63;