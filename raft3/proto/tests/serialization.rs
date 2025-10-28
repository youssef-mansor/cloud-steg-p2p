use proto::*;
use bincode;

#[test]
fn request_vote_roundtrip() {
    let msg = RpcMessage::RequestVote(RequestVote {
        term: 2,
        candidate_id: 1,
        last_log_index: 10,
        last_log_term: 1,
    });

    let encoded = bincode::serialize(&msg).unwrap();
    let decoded: RpcMessage = bincode::deserialize(&encoded).unwrap();
    assert_eq!(msg, decoded);
}

#[test]
fn append_entries_roundtrip() {
    let msg = RpcMessage::AppendEntries(AppendEntries {
        term: 3,
        leader_id: 1,
        prev_log_index: 5,
        prev_log_term: 2,
        entries: vec![LogEntry {
            term: 3,
            command: "set x=1".into(),
        }],
        leader_commit: 5,
    });

    let encoded = bincode::serialize(&msg).unwrap();
    let decoded: RpcMessage = bincode::deserialize(&encoded).unwrap();
    assert_eq!(msg, decoded);
}
