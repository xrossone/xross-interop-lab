//! 事件流（docs/05 §6 / CORE-07）：instance_id + 递增 sequence；
//! 有界缓冲（条数/字节双上限），淘汰后的 cursor 返回 gap + 快照建议，
//! 不假装完整重放。客户端恢复：先 snapshot 再接续。

use crate::limits::EventLimits;
use interop_contract::ids::SessionId;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub instance_id: String,
    pub sequence: u64,
    pub session_id: Option<SessionId>,
    pub kind: String,
    pub data: String,
    pub bytes: usize,
}

/// 订阅结果：要么是缓冲内的连续事件，要么是 gap（cursor 已被淘汰）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SubscribeResult {
    Events {
        events: Vec<Event>,
        next_cursor: u64,
    },
    /// cursor 指向的事件已被淘汰：返回快照建议序列号。
    Gap {
        snapshot_advice: u64,
    },
}

#[derive(Debug)]
pub struct EventBus {
    instance_id: String,
    buffer: std::collections::VecDeque<Event>,
    limits: EventLimits,
    total_bytes: usize,
    next_seq: u64,
}

impl EventBus {
    pub fn new(instance_id: &str, limits: EventLimits) -> Self {
        debug_assert!(limits.validate(), "EventLimits 非法");
        Self {
            instance_id: instance_id.to_string(),
            buffer: std::collections::VecDeque::new(),
            limits,
            total_bytes: 0,
            next_seq: 1,
        }
    }

    /// 发布一条事件，返回其 sequence。
    pub fn publish(
        &mut self,
        session_id: Option<&SessionId>,
        kind: &str,
        data: &str,
    ) -> u64 {
        let seq = self.next_seq;
        self.next_seq += 1;
        let event = Event {
            instance_id: self.instance_id.clone(),
            sequence: seq,
            session_id: session_id.cloned(),
            kind: kind.to_string(),
            data: data.to_string(),
            bytes: data.len() + kind.len() + 64, // 事件元数据估算开销
        };
        self.buffer.push_back(event);
        self.total_bytes += self.buffer.back().unwrap().bytes;
        // 双上限淘汰：任一超限即从最老开始丢弃
        while self.buffer.len() > self.limits.max_entries
            || self.total_bytes > self.limits.max_total_bytes
        {
            match self.buffer.pop_front() {
                Some(evicted) => self.total_bytes -= evicted.bytes,
                None => break,
            }
        }
        seq
    }

    /// 从 `after` 之后取事件。`after` 已被淘汰 → Gap + 快照建议。
    pub fn subscribe_after(&self, after: u64) -> SubscribeResult {
        let oldest = self
            .buffer
            .front()
            .map(|e| e.sequence)
            .unwrap_or(self.next_seq); // 空缓冲：无历史
        if after > 0 && after < oldest - 1 {
            return SubscribeResult::Gap {
                snapshot_advice: self.last_sequence(),
            };
        }
        let events: Vec<Event> = self
            .buffer
            .iter()
            .filter(|e| e.sequence > after)
            .cloned()
            .collect();
        let next_cursor = events.last().map(|e| e.sequence).unwrap_or(after);
        SubscribeResult::Events { events, next_cursor }
    }

    pub fn last_sequence(&self) -> u64 {
        self.buffer.back().map(|e| e.sequence).unwrap_or(0)
    }
}
