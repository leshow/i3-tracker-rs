# i3tracker

Originally forked from [i3-tracker-rs](https://github.com/danbruce/i3-tracker-rs). The code diverged as I explored async solutions.

This fork now runs completely inside a single thread using tokio. I re-wrote the underlying i3 library in order to get rid of the listen loop that was necessary for working with synchronous IO. This now runs inside of the main tokio runtime.
