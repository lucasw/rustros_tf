use tf_roslibrust::TfListener;
use roslibrust_codegen::Time;
use std::time::SystemTime;

use tf_roslibrust::tf_util;

roslibrust_codegen_macro::find_and_generate_ros_messages!();

/// Take in a source and destination frame argument
/// and repeatedly print the transform between them if any

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    use roslibrust::ros1::NodeHandle;

    // need to have leading slash on node name and topic to function properly
    // so figure out namespace then prefix it to name and topics
    let mut ns = String::from("");
    let args = std::env::args();
    {
        // get namespace
        for arg in args {
            if arg.starts_with("__ns:=") {
               ns = arg.replace("__ns:=", "");
            }
        }
    }

    let full_node_name = &format!("/{ns}/echo").replace("//", "/");
    println!("{}", format!("full ns and node name: {full_node_name}"));

    let nh = NodeHandle::new(&std::env::var("ROS_MASTER_URI")?, full_node_name)
        .await.unwrap();

    let frame1 = "map";
    let frame2 = "base_link";

    let mut listener = TfListener::new(&nh).await;
    // let mut dynamic_subscriber = nh.subscribe::<tf2_msgs::TFMessage>("/tf", 100).await.unwrap();

    let update_period = tokio::time::Duration::from_millis(1000);
    let mut next_update = SystemTime::now();

    println!("tf loop");
    loop {
        // sleep for remaining if nothing else interrupt select, or sleep for 0 seconds
        // if already past scheduled update
        let remaining = {
            let time_now = SystemTime::now();
            let remaining;
            if time_now > next_update {
                remaining = tokio::time::Duration::from_secs(0);
            } else {
                remaining = next_update.duration_since(time_now).unwrap();
            }
            remaining
        };

        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                println!("ctrl-c exiting");
                break;
            }
            // TODO(lucasw) move this into listener
            rv = listener._dynamic_subscriber.next() => {
                match rv {
                    Some(Ok(tfm)) => {
                        listener.update_tf(tfm).await;
                    },
                    Some(Err(error)) => {
                        panic!("{error}");
                    },
                    None => (),
                }
            }
            _ = tokio::time::sleep(remaining) => {
                next_update += update_period;
                println!("update {remaining:?}");
                // let lookup_stamp = Time {secs: 0, nsecs: 0};
                let lookup_stamp = tf_util::stamp_now();
                let tf = listener.lookup_transform(frame1, frame2, lookup_stamp.clone());
                // TODO(lucasw) header stamp is 0 when looking up most recent (with stamp 0)
                println!("{lookup_stamp:?} {tf:?}");
                let stamp_now = tf_util::stamp_now();
                println!("{stamp_now:?} {lookup_stamp:?} {tf:?}");
            }
        }
    }

    Ok(())
}
