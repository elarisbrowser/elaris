#pragma once

#include <elaris_ladybird.h>

extern "C" int32_t elaris_ladybird_test_create_private_view(ElarisLadybird*, uint64_t, ElarisLadybirdView**);
extern "C" int32_t elaris_ladybird_test_reset_private_session(ElarisLadybird*);
extern "C" int32_t elaris_ladybird_test_start_private_download(ElarisLadybirdView*, char const* url,
                                                               char const* filename);
extern "C" uint64_t elaris_ladybird_test_private_request_servers(ElarisLadybird*);
