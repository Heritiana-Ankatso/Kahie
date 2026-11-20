// C ABI over the Giac C++ API for nb-math's GiacCas (P0-07).
// One Giac context per process; calls are serialised by the Rust side.
#include <giac/config.h>
#include <giac/giac.h>
#include <cstring>
#include <string>
using namespace giac;

static context *ctx = nullptr;

extern "C" {
// Evaluate a Giac expression. Writes the result to *text and its LaTeX to *latex
// (malloc'd; free with giacshim_free). Returns 0 on success, 1 on error (message in *text).
static char *dup_str(const std::string &s) {
  char *p = (char *)malloc(s.size() + 1);
  memcpy(p, s.c_str(), s.size() + 1);
  return p;
}
int giacshim_eval2(const char *src, char **text, char **latex) {
  if (!ctx) ctx = new context;
  try {
    gen g(std::string(src), ctx);
    gen r = protecteval(g, 1, ctx);
    if (r.type == _STRNG && r.subtype == -1) {  // Giac error value
      *text = dup_str(r.print(ctx));
      *latex = dup_str("");
      return 1;
    }
    *text = dup_str(r.print(ctx));
    *latex = dup_str(gen2tex(r, ctx));
    return 0;
  } catch (std::exception &e) {
    *text = dup_str(e.what());
    *latex = dup_str("");
    return 1;
  }
}

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
