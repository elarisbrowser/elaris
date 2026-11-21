// SPDX-License-Identifier: GPL-3.0-only

// Adapted from Ladybird's UI/Qt/SpellCheckerMac.mm (BSD-2-Clause, Copyright (c) 2026-present, the Ladybird developers).

#include "spell_checker_macos.h"

#include <AK/AtomicRefCounted.h>
#include <AK/HashMap.h>
#include <AK/RefPtr.h>
#include <LibCore/EventLoop.h>
#include <LibWebView/StringConversionsMacOS.h>

#import <AppKit/AppKit.h>

// Inline predictions are private AppKit API. Each selector is checked with "respondsToSelector:" before use, so if
// AppKit drops one, predictions stop showing and nothing else changes.
@interface NSSpellChecker (ElarisInlinePredictions)
@property (class, readonly, getter=isAutomaticInlineCompletionEnabled) BOOL automaticInlineCompletionEnabled;
- (NSTextCheckingResult*)completionCandidateFromCandidates:(NSArray<NSTextCheckingResult*>*)candidates;
@end

namespace Elaris {

using namespace WebView;

// AppKit's spell checker asserts that it runs on the main queue, and the owner thread is not the main thread. Every
// call that touches it goes through here, so the main thread does the work and the owner thread waits for the answer.
static void run_on_main(void (^work)(void)) {
    if ([NSThread isMainThread])
        work();
    else
        dispatch_sync(dispatch_get_main_queue(), work);
}

// The private text checking type that asks for a single completion of the word at the caret.
static constexpr NSTextCheckingTypes text_checking_type_single_completion = 1ULL << 29;

// Answers come back on an arbitrary queue. The callbacks that take them stay on the owner thread, keyed by request, so
// only this atomically counted holder crosses threads.
struct PendingChecks : public AtomicRefCounted<PendingChecks> {
    HashMap<u64, Function<void(Vector<Web::SpellingError>)>> callbacks;
    u64 next_id { 0 };
};

struct PendingSuggestions : public AtomicRefCounted<PendingSuggestions> {
    HashMap<u64, Function<void(Optional<Web::WritingSuggestion>)>> callbacks;
    u64 next_id { 0 };
};

class MacSpellChecker final : public WebView::SpellChecker {
  public:
    virtual DocumentTag create_document_tag() override {
        __block DocumentTag tag = 0;
        run_on_main(^{
            tag = [NSSpellChecker uniqueSpellDocumentTag];
        });
        return tag;
    }

    virtual void close_document_tag(DocumentTag tag) override {
        run_on_main(^{
            [[NSSpellChecker sharedSpellChecker] closeSpellDocumentWithTag:tag];
        });
    }

    virtual void check(DocumentTag tag, Utf16String const& text, Function<void(Vector<Web::SpellingError>)> on_complete) override {
        auto id = m_pending_checks->next_id++;
        m_pending_checks->callbacks.set(id, move(on_complete));
        RefPtr<PendingChecks> pending_checks = m_pending_checks;
        RefPtr<Core::WeakEventLoopReference> event_loop = Core::EventLoop::current_weak();
        auto* string = utf16_string_to_ns_string(text);
        auto handler = ^(NSInteger, NSArray<NSTextCheckingResult*>* results, NSOrthography*, NSInteger) {
            Vector<Web::SpellingError> errors;
            for (NSTextCheckingResult* result in results) {
                if (result.resultType != NSTextCheckingTypeSpelling)
                    continue;
                errors.append({ static_cast<u32>(result.range.location), static_cast<u32>(result.range.length) });
            }
            auto strong_event_loop = event_loop->take();
            if (!strong_event_loop)
                return;
            strong_event_loop->deferred_invoke([pending_checks, id, errors = move(errors)]() mutable {
                if (auto callback = pending_checks->callbacks.take(id); callback.has_value())
                    (*callback)(move(errors));
            });
        };
        dispatch_async(dispatch_get_main_queue(), ^{
            [[NSSpellChecker sharedSpellChecker] requestCheckingOfString:string
                                                                   range:NSMakeRange(0, string.length)
                                                                   types:NSTextCheckingTypeSpelling
                                                                 options:nil
                                                  inSpellDocumentWithTag:tag
                                                       completionHandler:handler];
        });
    }

    virtual void guesses(DocumentTag tag, Utf16String const& word, Function<void(Vector<Utf16String>)> on_complete) override {
        auto* string = utf16_string_to_ns_string(word);
        __block NSArray<NSString*>* guesses = nil;
        run_on_main(^{
            guesses = [[NSSpellChecker sharedSpellChecker] guessesForWordRange:NSMakeRange(0, string.length)
                                                                      inString:string
                                                                      language:nil
                                                        inSpellDocumentWithTag:tag];
        });
        Vector<Utf16String> result;
        for (NSString* guess in guesses)
            result.append(ns_string_to_utf16_string(guess));
        on_complete(move(result));
    }

    virtual void ignore(DocumentTag tag, Utf16String const& word) override {
        auto* string = utf16_string_to_ns_string(word);
        run_on_main(^{
            [[NSSpellChecker sharedSpellChecker] ignoreWord:string inSpellDocumentWithTag:tag];
        });
    }

    // NB: The system dictionary learns the word for every app, so the tab's tag plays no part.
    virtual void learn(DocumentTag, Utf16String const& word) override {
        auto* string = utf16_string_to_ns_string(word);
        run_on_main(^{
            [[NSSpellChecker sharedSpellChecker] learnWord:string];
        });
    }

    virtual bool can_suggest() const override {
        __block BOOL available = NO;
        run_on_main(^{
            available = [NSSpellChecker respondsToSelector:@selector(isAutomaticInlineCompletionEnabled)]
                && [[NSSpellChecker sharedSpellChecker] respondsToSelector:@selector(completionCandidateFromCandidates:)];
        });
        return available;
    }

    virtual void suggest(DocumentTag tag, Utf16String const& text, u32 caret_offset, Function<void(Optional<Web::WritingSuggestion>)> on_complete) override {
        // The user's "Show inline predictive text" setting, which can change at any time.
        __block BOOL enabled = NO;
        run_on_main(^{
            enabled = NSSpellChecker.isAutomaticInlineCompletionEnabled;
        });
        if (!can_suggest() || !enabled) {
            on_complete({});
            return;
        }

        auto id = m_pending_suggestions->next_id++;
        m_pending_suggestions->callbacks.set(id, move(on_complete));
        RefPtr<PendingSuggestions> pending_suggestions = m_pending_suggestions;
        RefPtr<Core::WeakEventLoopReference> event_loop = Core::EventLoop::current_weak();
        auto* string = utf16_string_to_ns_string(text);
        auto handler = ^(NSInteger, NSArray<NSTextCheckingResult*>* candidates) {
            auto strong_event_loop = event_loop->take();
            if (!strong_event_loop)
                return;
            // The candidates arrive on an arbitrary queue, and the spellchecker picks among them on the owner thread.
            strong_event_loop->deferred_invoke([pending_suggestions, id, candidates]() mutable {
                Optional<Web::WritingSuggestion> suggestion;
                __block NSTextCheckingResult* candidate = nil;
                run_on_main(^{
                    candidate = [[NSSpellChecker sharedSpellChecker] completionCandidateFromCandidates:candidates];
                });
                if (candidate && candidate.replacementString && candidate.range.location != NSNotFound) {
                    suggestion = Web::WritingSuggestion {
                        static_cast<u32>(candidate.range.location),
                        static_cast<u32>(candidate.range.length),
                        ns_string_to_utf16_string(candidate.replacementString),
                    };
                }
                if (auto callback = pending_suggestions->callbacks.take(id); callback.has_value())
                    (*callback)(move(suggestion));
            });
        };
        // The platform's own candidate types go along with the completion type, which is how the system asks.
        NSTextCheckingTypes types = NSTextCheckingTypeSpelling | NSTextCheckingTypeReplacement | NSTextCheckingTypeCorrection | text_checking_type_single_completion;
        dispatch_async(dispatch_get_main_queue(), ^{
            [[NSSpellChecker sharedSpellChecker] requestCandidatesForSelectedRange:NSMakeRange(caret_offset, 0)
                                                                          inString:string
                                                                             types:types
                                                                           options:nil
                                                            inSpellDocumentWithTag:tag
                                                                 completionHandler:handler];
        });
    }

  private:
    NonnullRefPtr<PendingChecks> m_pending_checks { adopt_ref(*new PendingChecks) };
    NonnullRefPtr<PendingSuggestions> m_pending_suggestions { adopt_ref(*new PendingSuggestions) };
};

NonnullOwnPtr<WebView::SpellChecker> make_spell_checker_macos() {
    return make<MacSpellChecker>();
}

} // namespace Elaris
