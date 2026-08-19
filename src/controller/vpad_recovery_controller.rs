// Copyright 2026 Alexandre Mahdhaoui
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::adapter::vpad_adapter::VpadError;

pub fn create_pad_with_recovery<Pad, Open, Cleanup>(
    mut open: Open,
    mut remove_stray_pads: Cleanup,
) -> Result<Pad, VpadError>
where
    Open: FnMut() -> Result<Pad, VpadError>,
    Cleanup: FnMut() -> usize,
{
    match open() {
        Err(VpadError::Create) => match remove_stray_pads() {
            0 => Err(VpadError::Create),
            _ => open(),
        },
        result => result,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pad_that_opens_cleanly_never_touches_cleanup() {
        let mut cleanup_calls = 0;

        let result = create_pad_with_recovery(
            || Ok::<_, VpadError>("pad"),
            || {
                cleanup_calls += 1;
                0
            },
        );

        assert_eq!(result, Ok("pad"));
        assert_eq!(cleanup_calls, 0);
    }

    #[test]
    fn a_collision_triggers_cleanup_then_one_retry() {
        let mut opens = 0;

        let result = create_pad_with_recovery(
            || {
                opens += 1;

                match opens {
                    1 => Err(VpadError::Create),
                    _ => Ok("pad"),
                }
            },
            || 1,
        );

        assert_eq!(result, Ok("pad"));
        assert_eq!(opens, 2);
    }

    #[test]
    fn cleanup_finding_nothing_to_remove_does_not_retry() {
        let mut opens = 0;

        let result = create_pad_with_recovery(
            || {
                opens += 1;
                Err::<&str, _>(VpadError::Create)
            },
            || 0,
        );

        assert_eq!(result, Err(VpadError::Create));
        assert_eq!(opens, 1);
    }

    #[test]
    fn a_failure_that_is_not_a_collision_is_never_retried() {
        let mut opens = 0;
        let mut cleanup_calls = 0;

        let result = create_pad_with_recovery(
            || {
                opens += 1;
                Err::<&str, _>(VpadError::DriverAbsent)
            },
            || {
                cleanup_calls += 1;
                5
            },
        );

        assert_eq!(result, Err(VpadError::DriverAbsent));
        assert_eq!(opens, 1);
        assert_eq!(cleanup_calls, 0);
    }
}
