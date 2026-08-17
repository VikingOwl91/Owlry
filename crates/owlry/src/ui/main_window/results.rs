use super::*;

/// State for lazy loading results
#[derive(Default)]
pub(super) struct LazyLoadState {
    /// All matching results (may be more than displayed)
    pub(super) all_results: Vec<LaunchItem>,
    /// Number of items currently displayed
    pub(super) displayed_count: usize,
    /// The query that produced these results (for highlighting in lazy-loaded batches)
    pub(super) query: String,
}

/// Number of items to display initially and per batch
pub(super) const INITIAL_RESULTS: usize = 15;
const LOAD_MORE_BATCH: usize = 10;

impl MainWindow {
    /// Schedule initial results population via idle callback.
    /// Call this AFTER `window.present()` so the window appears immediately.
    pub fn schedule_initial_results(&self) {
        let backend = self.backend.clone();
        let results_list = self.results_list.clone();
        let config = self.config.clone();
        let filter = self.filter.clone();
        let current_results = self.current_results.clone();
        let lazy_state = self.lazy_state.clone();

        let scheduled_at = std::time::Instant::now();
        gtk4::glib::idle_add_local_once(move || {
            log::debug!(
                "startup: initial-results idle fired +{:.1?} after schedule",
                scheduled_at.elapsed()
            );
            let cfg = config.borrow();
            let max_results = cfg.general.max_results;
            drop(cfg);

            let query_start = std::time::Instant::now();
            let results =
                backend
                    .borrow_mut()
                    .search("", max_results, &filter.borrow(), &config.borrow());
            log::debug!(
                "startup: initial daemon query returned {} items in {:.1?}",
                results.len(),
                query_start.elapsed()
            );

            // Clear existing results
            results_list.remove_all();

            let initial_count = INITIAL_RESULTS.min(results.len());

            for item in results.iter().take(initial_count) {
                let row = ResultRow::new(item, "");
                results_list.append(&row);
            }

            if let Some(first_row) = results_list.row_at_index(0) {
                results_list.select_row(Some(&first_row));
            }

            *current_results.borrow_mut() = results[..initial_count].to_vec();
            let mut lazy = lazy_state.borrow_mut();
            lazy.all_results = results;
            lazy.displayed_count = initial_count;
        });
    }

    /// Set up lazy loading scroll detection
    pub(super) fn setup_lazy_loading(&self) {
        let vadj = self.scrolled.vadjustment();
        let results_list = self.results_list.clone();
        let lazy_state = self.lazy_state.clone();
        let current_results = self.current_results.clone();

        // Load more on scroll
        vadj.connect_value_changed(move |adj| {
            let value = adj.value();
            let upper = adj.upper();
            let page_size = adj.page_size();

            // Load more when near bottom (within 50px)
            let near_bottom = upper > page_size && (value + page_size >= upper - 50.0);
            if near_bottom {
                Self::load_more_items(&lazy_state, &results_list, &current_results);
            }
        });

        // Also load more when selecting rows near the end (keyboard navigation)
        let lazy_state2 = self.lazy_state.clone();
        let results_list2 = self.results_list.clone();
        let current_results2 = self.current_results.clone();

        self.results_list.connect_row_selected(move |_, row| {
            if let Some(row) = row {
                let index = row.index();
                let lazy = lazy_state2.borrow();
                let displayed = lazy.displayed_count;
                let all_count = lazy.all_results.len();
                drop(lazy);

                // Load more if within 3 items of the end
                if displayed < all_count && (index as usize) >= displayed.saturating_sub(3) {
                    Self::load_more_items(&lazy_state2, &results_list2, &current_results2);
                }
            }
        });
    }

    /// Load more items from lazy state
    fn load_more_items(
        lazy_state: &Rc<RefCell<LazyLoadState>>,
        results_list: &ListBox,
        current_results: &Rc<RefCell<Vec<LaunchItem>>>,
    ) {
        let mut lazy = lazy_state.borrow_mut();
        let all_count = lazy.all_results.len();
        let displayed = lazy.displayed_count;

        if displayed < all_count {
            // Load next batch
            let new_end = (displayed + LOAD_MORE_BATCH).min(all_count);
            let query = lazy.query.clone();
            for item in lazy.all_results[displayed..new_end].iter() {
                let row = ResultRow::new(item, &query);
                results_list.append(&row);
            }
            lazy.displayed_count = new_end;

            // Update current_results
            let mut current = current_results.borrow_mut();
            current.extend(lazy.all_results[displayed..new_end].iter().cloned());
        }
    }
}
