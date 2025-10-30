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
    pub votes_received: usize,
    pub peers_count: usize,
}

impl NodeState {
    pub fn new() -> Self {
        Self {
            current_term: 0,
            voted_for: None,
            role: Role::Follower,
            votes_received: 0,
            peers_count: 0,
        }
    }

    /// Handle a RequestVote RPC and return a response.
    pub fn handle_request_vote(&mut self, req: &RequestVote) -> RequestVoteResponse {
        // Step 1: update term if needed
        if req.term > self.current_term {
            self.current_term = req.term;
            self.voted_for = None;
            self.role = Role::Follower;
        }

        // Step 2: grant vote if term is up-to-date and we haven't voted yet
        let vote_granted = if req.term == self.current_term
            && (self.voted_for.is_none() || self.voted_for == Some(req.candidate_id))
        {
            self.voted_for = Some(req.candidate_id);
            true
        } else {
            false
        };

        RequestVoteResponse {
            term: self.current_term,
            vote_granted,
        }
    }

    /// ✅ Handle AppendEntries RPC (heartbeats / log replication)
    pub fn handle_append_entries(&mut self, req: &crate::AppendEntries) -> crate::AppendEntriesResponse {
        // Reject outdated terms
        if req.term < self.current_term {
            return crate::AppendEntriesResponse {
                term: self.current_term,
                success: false,
            };
        }

        // Accept heartbeat / append entries
        self.current_term = req.term;
        self.role = Role::Follower;
        self.voted_for = Some(req.leader_id);

        crate::AppendEntriesResponse {
            term: self.current_term,
            success: true,
        }
    }

    /// Start a new election for `candidate_id`.
    pub fn start_election(&mut self, id: NodeId) -> Term {
        self.current_term += 1;
        self.voted_for = Some(id);
        self.role = Role::Candidate;
        self.votes_received = 1; // vote for self
        println!("[Node {}] started election for term {}", id, self.current_term);
        self.current_term
    }

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
