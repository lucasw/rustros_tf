use roslibrust_codegen::{Duration, Time};

use crate::{
    tf_error::TfError,
    transforms::{geometry_msgs::TransformStamped, interpolate, to_transform_stamped},
};

// TODO(lucasw) is there a reason Duration has 'sec' and Time has 'nsecs'?
fn get_duration_seconds(dur: Duration) -> f64 {
    f64::from(dur.sec) + f64::from(dur.nsec) / 1e9
}

fn binary_search_time(chain: &[TransformStamped], time: Time) -> Result<usize, usize> {
    chain.binary_search_by(|element| element.header.stamp.cmp(&time))
}

#[derive(Clone, Debug)]
pub(crate) struct TfIndividualTransformChain {
    cache_duration: Duration,
    static_tf: bool,
    // TODO: Implement a circular buffer. Current method is slow.
    pub(crate) transform_chain: Vec<TransformStamped>,
}

impl TfIndividualTransformChain {
    pub(crate) fn new(static_tf: bool, cache_duration: Duration) -> Self {
        Self {
            cache_duration,
            transform_chain: Vec::new(),
            static_tf,
        }
    }

    fn newest_stamp(&self) -> Option<Time> {
        self.transform_chain.last().map(|x| x.header.stamp)
    }

    pub(crate) fn add_to_buffer(&mut self, msg: TransformStamped) {
        let index = binary_search_time(&self.transform_chain, msg.header.stamp)
            .unwrap_or_else(|index| index);
        self.transform_chain.insert(index, msg);

        if let Some(newest_stamp) = self.newest_stamp() {
            if newest_stamp > (Time {secs: 0, nsecs: 0}) + self.cache_duration {
                let time_to_keep = newest_stamp - self.cache_duration;
                let index =
                    binary_search_time(&self.transform_chain, time_to_keep).unwrap_or_else(|x| x);
                self.transform_chain.drain(..index);
            }
        }
    }

    /// If timestamp is zero, return the latest transform.
    pub(crate) fn get_closest_transform(
        &self,
        time: Time,
    ) -> Result<TransformStamped, TfError> {
        if time.seconds() == 0.0 {
            return Ok(self.transform_chain.last().unwrap().clone());
        }

        if self.static_tf {
            return Ok(self.transform_chain.last().unwrap().clone());
        }

        match binary_search_time(&self.transform_chain, time) {
            Ok(x) => return Ok(self.transform_chain.get(x).unwrap().clone()),
            Err(x) => {
                if x == 0 {
                    return Err(TfError::AttemptedLookupInPast(
                        time,
                        Box::new(self.transform_chain.first().unwrap().clone()),
                    ));
                }
                if x >= self.transform_chain.len() {
                    return Err(TfError::AttemptedLookUpInFuture(
                        Box::new(self.transform_chain.last().unwrap().clone()),
                        time,
                    ));
                }
                let tf1 = self.transform_chain.get(x - 1).unwrap().clone().transform;
                let tf2 = self.transform_chain.get(x).unwrap().clone().transform;
                let time1 = self.transform_chain.get(x - 1).unwrap().header.stamp;
                let time2 = self.transform_chain.get(x).unwrap().header.stamp;
                let header = self.transform_chain.get(x).unwrap().header.clone();
                let child_frame = self.transform_chain.get(x).unwrap().child_frame_id.clone();
                // interpolate between the timestamps that bracket the desired time
                let total_duration = time2 - time1;
                let desired_duration = time - time1;
                let weight = 1.0 - get_duration_seconds(desired_duration) / get_duration_seconds(total_duration);
                let final_tf = interpolate(tf1, tf2, weight);
                let ros_msg = to_transform_stamped(final_tf, header.frame_id, child_frame, time);
                Ok(ros_msg)
            }
        }
    }

    pub(crate) fn has_valid_transform(&self, time: Time) -> bool {
        if self.transform_chain.is_empty() {
            return false;
        }

        if self.static_tf {
            return true;
        }

        let first = self.transform_chain.first().unwrap();
        let last = self.transform_chain.last().unwrap();

        time.seconds() == 0.0 || (time >= first.header.stamp && time <= last.header.stamp)
    }
}
