/* Test fixture — the *consumer-provided* dispatch seam.
 *
 * ingot does not ship this file; a real consumer writes its own, binding
 * send_tinyfsm_event to its own tinyfsm::FsmList. This minimal version
 * exists only so the generated dm_key_events_wrapper.cpp can be compiled
 * and linked in ingot's own test suite. */
#ifndef INGOT_TEST_FSMLIST_HPP
#define INGOT_TEST_FSMLIST_HPP

#include "tinyfsm.hpp"

/* A minimal single-state FSM. The catch-all react(tinyfsm::Event const &)
 * absorbs every generated FSM_EVENT_* struct without per-event overloads,
 * since all of them derive from tinyfsm::Event. */
struct DummyFsm : tinyfsm::Fsm<DummyFsm>
{
    void entry() { }
    void exit() { }
    void react(tinyfsm::Event const &) { }
};

using fsm_list = tinyfsm::FsmList<DummyFsm>;

/* The seam the generated wrapper calls: fan a typed event to every FSM. */
template<typename E>
void send_tinyfsm_event(E const & event)
{
    fsm_list::template dispatch<E>(event);
}

#endif /* INGOT_TEST_FSMLIST_HPP */
