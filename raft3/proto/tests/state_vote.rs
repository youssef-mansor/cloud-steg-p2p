use proto::{state::NodeState, RequestVote};

#[test]
fn test_handle_request_vote_basic() {
    let mut s = NodeState::new();

    let req = RequestVote {
        term: 1,
        candidate_id: 42,
        last_log_index: 0,
        last_log_term: 0,
    };

    let resp = s.handle_request_vote(&req);

    assert!(resp.vote_granted, "expected vote to be granted");
    assert_eq!(resp.term, 1);
    assert_eq!(s.voted_for, Some(42));
}

#[test]
fn test_handle_request_vote_reject_second_vote_same_term() {
    let mut s = NodeState::new();

    let req1 = RequestVote {
        term: 2,
        candidate_id: 1,
        last_log_index: 0,
        last_log_term: 0,
    };
    let _ = s.handle_request_vote(&req1);
    assert_eq!(s.voted_for, Some(1));

    let req2 = RequestVote {
        term: 2,
        candidate_id: 2,
        last_log_index: 0,
        last_log_term: 0,
    };
    let resp2 = s.handle_request_vote(&req2);
    assert!(!resp2.vote_granted, "expected rejection for same-term second vote");
    assert_eq!(s.voted_for, Some(1));
}
