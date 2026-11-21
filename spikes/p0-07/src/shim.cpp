// Minimal C ABI over the Giac C++ API (spike P0-07). One context per process.
#include <giac/config.h>
#include <giac/giac.h>
#include <cstring>
#include <string>
using namespace giac;

static context *ctx = nullptr;

extern "C" {
// Evaluate a Giac expression; returns a malloc'd string (caller frees with giacshim_free).
char *giacshim_eval(const char *src) {
  if (!ctx) ctx = new context;
  std::string out;
  try {
    gen g(std::string(src), ctx);
    gen r = protecteval(g, 1, ctx);
    out = r.print(ctx);
  } catch (std::exception &e) {
    out = std::string("ERROR: ") + e.what();
  }
  char *s = (char *)malloc(out.size() + 1);
  memcpy(s, out.c_str(), out.size() + 1);
  return s;
}
void giacshim_free(char *s) { free(s); }
}
