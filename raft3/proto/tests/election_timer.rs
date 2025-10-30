use proto::state::{NodeState, Role};
use tokio::time::{sleep, Duration};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Quick unit test for start_election (synchronous)
#[test]
fn start_election_increments_term_and_sets_candidate() {
    let mut s = NodeState::new();
    assert_eq!(s.current_term, 0);
    assert!(s.is_follower());

    let new_term = s.start_election(7);
    assert_eq!(new_term, 1);
    assert_eq!(s.current_term, 1);
    assert!(s.is_candidate());
    assert_eq!(s.voted_for, Some(7));
}

/// Simulate an election timeout using tokio::time — async test
#[tokio::test]
async fn timeout_triggers_start_election() {
    // small deterministic bounds for test
    let min = Duration::from_millis(20);
    let max = Duration::from_millis(50);

    let state = Arc::new(Mutex::new(NodeState::new()));

    // Spawn a task that waits a randomized timeout then starts election.
    let s_clone = state.clone();
    let timer_task = tokio::spawn(async move {
        // simulate randomized timeout in [min,max]
        let rand_ms = {
            // use a simple deterministic pseudo-random based on time to avoid adding rand crate
            let nanos = std::time::Instant::now().elapsed().subsec_nanos() as u64;
            let range = (max - min).as_millis() as u64 + 1;
            let nanos_mod = nanos % range;
            min + Duration::from_millis(nanos_mod)
        };
        sleep(rand_ms).await;

        let mut st = s_clone.lock().await;
        st.start_election(1);
    });

    // Wait longer than max to ensure the timer has fired
    sleep(max + Duration::from_millis(20)).await;

    // Ensure timer task finished
    timer_task.await.unwrap();

    let st = state.lock().await;
    assert!(st.is_candidate(), "expected node to become candidate after timeout");
    assert_eq!(st.current_term, 1);
    assert_eq!(st.voted_for, Some(1));
}
