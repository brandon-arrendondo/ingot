/* Test fixture driver — proves the generated C++ event artifacts compile,
 * link, and resolve the consumer dispatch seam against real tinyfsm.
 *
 * Compiling dm_key_events_wrapper.cpp instantiates send_tinyfsm_event<E>
 * for every FSM_EVENT_* struct (one per event key), so a successful build
 * already exercises every switch case. main() then links and runs the
 * dispatch entry point to confirm the seam is satisfied end-to-end. */
#include "tinyfsm.hpp"
#include "fsmlist.hpp"
#include "dm_key_events_wrapper.hpp"

FSM_INITIAL_STATE(DummyFsm, DummyFsm)

int main(void)
{
    fsm_list::start();
    /* key_id 0 falls through to the default branch — model-agnostic, just
     * forces the wrapper to link. The switch cases are already compiled. */
    send_tinyfsm_event_by_key(0U);
    return 0;
}
