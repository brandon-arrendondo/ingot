/* Test fixture driver — proves the generated C++ event artifacts compile,
 * link, and resolve the consumer dispatch seam against real tinyfsm.
 *
 * Compiling dm_key_events_wrapper.cpp instantiates send_tinyfsm_event<E>
 * for every FSM_EVENT_* struct (one per event key), so a successful build
 * already exercises every switch case. main() then links and runs the
 * dispatch entry point to confirm the seam is satisfied end-to-end.
 *
 * dm_key_has_event() is checked at run time against examples/full.toml, the
 * model `invoke test` generates this fixture from: an event key must answer
 * true, a non-event key and an id that is no key must answer false.
 *
 * dm_event_list is instantiated with consumer-shaped KeyEvent / KeyList
 * templates defined here, the way a composition root instantiates it with its
 * own: every entry must name a real key id and a tinyfsm event type. */
#include <cstddef>
#include <cstdint>
#include <type_traits>

#include "tinyfsm.hpp"
#include "fsmlist.hpp"
#include "dm_key_events_wrapper.hpp"
#include "dm_key_has_event.hpp"
#include "dm_key_event_list.hpp"

FSM_INITIAL_STATE(DummyFsm, DummyFsm)

/** @brief A consumer's (key id, event type) pair. @version 1 */
template<std::uint32_t Id, class E>
struct KeyEvent
{
    static constexpr std::uint32_t id = Id;
    using event = E;
};

/** @brief A consumer's list: counts its entries and checks each is a tinyfsm event. @version 1 */
template<class... KE>
struct KeyList;

template<>
struct KeyList<>
{
    static constexpr std::size_t size = 0U;
    static constexpr bool all_events = true;
};

template<class Head, class... Tail>
struct KeyList<Head, Tail...>
{
    static constexpr std::size_t size = 1U + KeyList<Tail...>::size;
    static constexpr bool all_events =
        std::is_base_of<tinyfsm::Event, typename Head::event>::value && KeyList<Tail...>::all_events;
};

using Keys = dm_event_list<KeyEvent, KeyList>;
static_assert(Keys::size == dm_event_count, "dm_event_list size must equal dm_event_count");
static_assert(Keys::size > 0U, "fixture model must carry event keys");
static_assert(Keys::all_events, "every dm_event_list entry must name a tinyfsm event");

/**
 * @brief   Check dm_key_has_event() on one key of each kind from examples/full.toml.
 * @version 1
 * @return  0 when every answer is right, 1 otherwise.
 */
static int check_has_event(void)
{
    int failures = 0;
    if (!dm_key_has_event(DM_KEY_APPLIANCE_STATUS_MODE)) {
        failures++; /* event = true */
    }
    if (dm_key_has_event(DM_KEY_APPLIANCE_STATUS_TEMPERATURE)) {
        failures++; /* event = false */
    }
    if (dm_key_has_event(0U)) {
        failures++; /* not a key of this model */
    }
    return (failures == 0) ? 0 : 1;
}

/**
 * @brief   Start the FSM list, drive the dispatch seam once, then check the predicate.
 * @version 2
 * @return  process exit status: 0 on success.
 */
int main(void)
{
    fsm_list::start();
    /* key_id 0 falls through to the default branch — model-agnostic, just
     * forces the wrapper to link. The switch cases are already compiled. */
    send_tinyfsm_event_by_key(0U);
    return check_has_event();
}
