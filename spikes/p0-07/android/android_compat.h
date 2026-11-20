/* Force-included (-include) when building Giac for Android.
 *
 * Bionic deliberately has no pthread cancellation. Giac uses it only to kill a separate
 * evaluation thread on timeout/interrupt; Kahie's shim evaluates on the calling thread, so
 * these are no-ops. (A runaway computation is bounded by Giac's own TIMEOUT checks instead.)
 */
#ifndef KAHIE_ANDROID_COMPAT_H
#define KAHIE_ANDROID_COMPAT_H
#if defined(__ANDROID__)
#include <pthread.h>
#ifndef PTHREAD_CANCEL_ENABLE
#define PTHREAD_CANCEL_ENABLE 0
#define PTHREAD_CANCEL_DISABLE 1
#define PTHREAD_CANCEL_DEFERRED 0
#define PTHREAD_CANCEL_ASYNCHRONOUS 1
static inline int pthread_setcancelstate(int state, int *old) { (void)state; if (old) *old = 0; return 0; }
static inline int pthread_setcanceltype(int type, int *old) { (void)type; if (old) *old = 0; return 0; }
static inline int pthread_cancel(pthread_t t) { (void)t; return 0; }
#endif
#endif
#endif
