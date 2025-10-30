use crate::{NodeId, RequestVote, RequestVoteResponse, Term};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Role {
    Follower,
    Candidate,
    Leader,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeState {
    pub current_term: Term,
    pub voted_for: Option<NodeId>,
    pub role: Role,
}

impl NodeState {
    pub fn new() -> Self {
        Self {
            current_term: 0,
            voted_for: None,
            role: Role::Follower,
        }
    }

    /// Handle a RequestVote RPC and return a response.
    pub fn handle_request_vote(&mut self, req: &RequestVote) -> RequestVoteResponse {
        // If candidate has higher term, update.
        if req.term > self.current_term {
            self.current_term = req.term;
            self.voted_for = None;
            self.role = Role::Follower;
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

    /// Start a new election for `candidate_id`.
    /// - increments term
    /// - becomes Candidate
    /// - records vote for self (candidate_id)
    /// Returns the new term.
    pub fn start_election(&mut self, candidate_id: NodeId) -> Term {
        self.current_term = self.current_term.saturating_add(1);
        self.role = Role::Candidate;
        self.voted_for = Some(candidate_id);
        self.current_term
    }

    /// Mark this node as leader.
    pub fn become_leader(&mut self) {
        self.role = Role::Leader;
    }

    pub fn is_follower(&self) -> bool {
        self.role == Role::Follower
    }

    pub fn is_candidate(&self) -> bool {
        self.role == Role::Candidate
    }

    pub fn is_leader(&self) -> bool {
        self.role == Role::Leader
    }
}
