//! Prints a policy's logits for the fixed probe input of train/export.py, to check
//! that this implementation and PyTorch agree. Usage: probe POLICY.swp
use swarm_sim::{Net, OBS_LEN};

fn main() -> std::io::Result<()> {
    let path = std::env::args().nth(1).expect("usage: probe POLICY.swp");
    let net = Net::load(&path)?;
    let probe: Vec<f32> = (0..OBS_LEN).map(|i| (i as f32 * 0.37).sin()).collect();
    // Fed twice, as export.py does: the second answer depends on what the first left in the memory.
    let mut state = net.blank();
    net.logits(&probe, &mut state);
    let logits: Vec<String> = net.logits(&probe, &mut state).iter().map(|v| format!("{v:.4}")).collect();
    println!("probe logits: {}", logits.join(" "));
    Ok(())
}
