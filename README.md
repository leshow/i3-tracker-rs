# i3tracker (nightly)

Originally forked from [i3-tracker-rs](https://github.com/danbruce/i3-tracker-rs). The code diverged as I explored async solutions.

This fork now runs completely inside a single thread using tokio. I re-wrote the underlying i3 library in order to get rid of the listen loop that was necessary for working with synchronous IO. This now runs inside of the main tokio runtime.

The timeout ticker that would spawn a thread each time you switched windows in the original version is also removed-- the tick happens courtesy of `tokio_timer`

**update Sep 2019**: using async/await
