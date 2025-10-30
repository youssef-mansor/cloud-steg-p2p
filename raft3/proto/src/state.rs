use crate::{NodeId, RequestVote, RequestVoteResponse, Term};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeState {
    pub current_term: Term,
    pub voted_for: Option<NodeId>,
}

impl NodeState {
    pub fn new() -> Self {
        Self {
            current_term: 0,
            voted_for: None,
        }
    }

    /// Handle a RequestVote RPC and return a response.
    pub fn handle_request_vote(&mut self, req: &RequestVote) -> RequestVoteResponse {
        // Update term if candidate’s term is newer
        if req.term > self.current_term {
            self.current_term = req.term;
            self.voted_for = None;
        }

        let can_vote = (self.voted_for.is_none() || self.voted_for == Some(req.candidate_id))
            && req.term >= self.current_term;

        if can_vote {
            self.voted_for = Some(req.candidate_id);
        }

        RequestVoteResponse {
            term: self.current_term,
            vote_granted: can_vote,
        }
    }
}
