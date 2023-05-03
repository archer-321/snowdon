// Copyright © 2023 Archer <archer@nefarious.dev>
// Licensed under the Apache License, Version 2.0 (the "Licence");
// you may not use this file except in compliance with the Licence.
// You may obtain a copy of the Licence at
//     https://www.apache.org/licenses/LICENSE-2.0
// Unless required by applicable law or agreed to in writing, software
// distributed under the Licence is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the Licence for the specific language governing permissions and
// limitations under the Licence.

// This model uses a reduced snowflake type that only consists of 32 bits (`int`). Moreover, we
// exclude the instance ID part, as that's defined to be constant for every ID generated on the same
// machine.
//
// Our resulting snowflake consists of a 16-bit timestamp followed by a 16-bit sequence number.

// Global state variable holding the last generated snowflake.
int state;
// A variable holding the current (simulated) timestamp.
int current_time;

// A global array allowing us to verify properties of the generated snowflakes across all processes
int snowflakes[5];
// The number of processes that have finished execution.
int finished;

// The processes creating snowflakes.
//
// When changing the number of processes here, also update the size of the `snowflakes` array above
// and the number of finished processes required at the end of this process specification.
active [3] proctype Snowflake() {
    int old_state, old_timestamp, old_sequence, timestamp, sequence, new_snowflake, i, j;
    bool no_hole;

    do
    ::
        // Atomically load the current state
        old_state = state;

        // Obtain a timestamp. We obtain the timestamp *after* loading the old state, as this allows
        // us to detect non-monotonous system clocks. If we'd load this variable before we load the
        // state, another thread could interleave and update the state variable with a timestamp in
        // the future. The code below would incorrectly interpret this as a non-monotonous clock,
        // then.
        timestamp = current_time;

        // Extract the timestamp and sequence number from the loaded state
        d_step {
            old_timestamp = (old_state >> 16) & 65535;
            old_sequence = old_state & 65535;
        }

        // The behaviour in the block below is fully deterministic, and threads interleaving at any
        // point can't change this process' behaviour, as we only read and write local variables.
        //
        // For performance reasons, we'll wrap this in a `d_step` block to reduce the total number
        // of states that SPIN has to validate.
        d_step {
            // Create the new snowflake
            if
            ::  timestamp > old_timestamp ->
                // The timestamp increased, so reset the sequence number
                sequence = 0;
            ::  timestamp == old_timestamp ->
                // The timestamp didn't change, so increment the sequence number
                sequence = old_sequence + 1;
                if
                ::  (sequence & 65535) == 0 ->
                    // Normally, we'd return an error indicating that this operation can be retried
                    // in the next millisecond. Our model implementation doesn't generate enough IDs
                    // to ever overflow the counter, however, so this code should be unreachable.
                    assert(false);
                ::  else ->
                    skip;
                fi
            ::  else ->
                // Non-monotonous clock - Our actual implementation would return an error here, but
                // the clock implementation in this model is strictly monotonous, so we (hopefully)
                // won't end up here
                assert(false);
            fi
            new_snowflake = (timestamp << 16) | sequence;
        }

        // At this point, we have a valid snowflake available as `(timestamp, sequence)`. Verify that
        // no other thread has since generated another snowflake.

        // The atomic block below can be implemented with a CAS operation
        atomic {
            if
            ::  state == old_state ->
                state = new_snowflake;
                break;
            :: else ->
                skip;
            fi
        }
    od

    // Return (store) the newly generated snowflake. `_pid` starts at 0 and is incremented for
    // every other process that's created. As this is the first process specification, the first
    // instance of `Snowflake` has its `_pid` set to 0, so we can use this as the array index.
    // This atomic block is not strictly necessary but further reduces the number of states SPIN
    // has to check.
    atomic {
        snowflakes[_pid] = new_snowflake;
        finished = finished + 1;
    }

    // If we're the last process, validate all stored snowflakes.
    // Again, we use an `atomic` block here to reduce the number of states.
    atomic {
        if
        ::  finished == 3 ->
            i = 0;
            do
            ::  if
                ::  i < 3 ->
                    // Naively check for duplicates - we don't need to be efficient here, as this
                    // code will only run once on a relatively short list of IDs
                    j = i + 1;
                    do
                    ::  if
                        ::  j < 3 ->
                            assert(snowflakes[i] != snowflakes[j]);
                            j = j + 1;
                        ::  else ->
                            break;
                        fi
                    od

                    // Naively verify that we don't have any "holes" in our generated IDs (e.g. a
                    // snowflake (t,s) = (1, 1) but no (1, 0)).
                    if
                    ::  (snowflakes[i] & 65535) != 0 ->
                        // The snowflake has a sequence number != 0, so a snowflake with the same
                        // timestamp and the sequence number n - 1 must exist
                        j = 0;
                        no_hole = false;
                        do
                        ::  if
                            ::  j != i && j < 3 ->
                                // Unfortunately, we can't move the `==` and `&&` to the next line,
                                // as SPIN reports this as a syntax error. To keep our max-width of
                                // 100 characters, we need to use this sub-optimal formatting :c
                                if
                                ::  (snowflakes[j] >> 16) & 65535 ==
                                    (snowflakes[i] >> 16) & 65535 &&
                                    (snowflakes[j] & 65535) == ((snowflakes[i] & 65535) - 1) ->
                                    no_hole = true;
                                    break;
                                ::  else ->
                                    skip;
                                fi
                            ::  j == i ->
                                skip;
                            ::  else ->
                                break;
                            fi
                        od
                        assert(no_hole);
                    ::  else ->
                        skip;
                    fi
                    i = i + 1;
                ::  else ->
                    break;
                fi
            od
        ::  else ->
            skip;
        fi
    }
}

// A naive time implementation.
//
// Note that we're not testing non-monotonous system clock behaviour with this model. Refer to the
// "Non-monotonous clock" comment and the "Obtain a timestamp" comment in Snowflake for an overview
// of how we (try to) detect this in the actual program.
active [5] D_proctype Time() {
    current_time = current_time + 1;
}
