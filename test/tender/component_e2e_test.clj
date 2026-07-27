(ns tender.component-e2e-test
  "Real compiler → Component → Kototama → Aiueos provider integration.

   This is deliberately opt-in because it needs the Rust micro-TCB executable.
   CI enables it after building that executable; ordinary Clojure unit tests
   retain their fast, host-independent path."
  (:require [clojure.test :refer [deftest is testing]]
            [kotoba.abi.contract :as abi]
            [kotoba.compiler.core :as compiler]
            [kototama.aiueos-adapter :as adapter]
            [tender.component :as component]
            [multiformats.core :as mf])
  (:import [java.io File]
           [java.security MessageDigest]))

(defn- sha256-file [^File file]
  (let [digest (MessageDigest/getInstance "SHA-256")]
    (with-open [input (java.io.FileInputStream. file)]
      (let [buffer (byte-array 8192)]
        (loop [n (.read input buffer)]
          (when (pos? n)
            (.update digest buffer 0 n)
            (recur (.read input buffer))))))
    (apply str (map #(format "%02x" (bit-and (int %) 0xff)) (.digest digest)))))

(defn- component-world [bytes]
  {:target abi/component-target :wasi-version abi/wasi-version :profile :sync
   :imports #{} :exports #{:app/main} :grants #{} :provider-bindings {}
   :abilities {} :ambient-wasi abi/ambient-wasi?
   :budgets {:fuel 100000 :memory-pages 4 :deadline-ms 10000}
   :identity {:component-cid (mf/cidv1-raw bytes)
              :package-lock-cid (mf/cidv1-raw (.getBytes "e2e-lock" "UTF-8"))
              :definition-cids #{(mf/cidv1-raw (.getBytes "e2e-definition" "UTF-8"))}}})

(defn- typed-component-world [bytes host-sha256]
  {:target abi/component-target-v2 :wasi-version abi/wasi-version :profile :sync
   :imports #{} :exports #{:app/main} :grants #{} :provider-bindings {}
   :abilities {} :ambient-wasi abi/ambient-wasi?
   :budgets {:fuel 100000 :memory-pages 4 :deadline-ms 10000}
   :runtime-bindings {:component-host-sha256 host-sha256}
   :identity {:component-cid (mf/cidv1-raw bytes)
              :package-lock-cid (mf/cidv1-raw (.getBytes "v3-e2e-lock" "UTF-8"))
              :definition-cids
              #{(mf/cidv1-raw (.getBytes "v3-e2e-definition" "UTF-8"))}}})

(defn- cid [text]
  (mf/cidv1-raw (.getBytes ^String text "UTF-8")))

(defn- execution-identity [component-bytes]
  {:format :kotoba.execution-identity/v1
   :plan-cid (cid "e2e-plan") :code-closure-cid (cid "e2e-closure")
   :artifact-cid (cid "e2e-artifact")
   :compiler-contract (cid "e2e-compiler-contract")
   :component-cid (mf/cidv1-raw component-bytes)
   :wit-world-cid (mf/cidv1-raw (.getBytes ^String (abi/world-wit [7]) "UTF-8"))
   :package-lock-cid (cid "e2e-package-lock")
   :policy-cid (cid "e2e-policy")
   :policy-decision-cid (cid "e2e-decision")
   :db-basis (cid "e2e-basis")
   :grant-cids [(cid "e2e-grant")]
   :approval-cids [(cid "e2e-approval")]
   ;; This identifies the common Component host contract. Concrete host bytes
   ;; remain separately pinned in each full host receipt.
   :runtime-identity (cid "kototama-component-host-contract-v1")
   :input-cid (cid "e2e-input") :outcome-cid (cid "e2e-outcome")
   :host-receipt-cids [(cid "e2e-host-receipt-set")]})

(defn- run-on-host [artifact host runtime providers opts]
  (component/admit-and-run-with-aiueos!
   artifact
   (assoc (component-world (:bytes artifact))
          :budgets (merge (:budgets artifact) {:deadline-ms 10000}))
   (:bytes artifact) providers
   (merge {:runtime runtime
           :component-host (.getAbsolutePath ^File host)
           :component-host-sha256 (sha256-file host)
           :execution-identity (execution-identity (:bytes artifact))
           :lease-epoch 1 :lease-ttl-ms 10000
           :lease-id "component-cross-host-lease"
           :now-ms 1000}
          opts)))

(deftest ^:integration compiler-component-aiueos-provider-round-trip
  (if-let [host-path (System/getenv "KOTOTAMA_COMPONENT_HOST")]
    (let [host (File. host-path)
          jco-path (System/getenv "KOTOTAMA_JCO_COMPONENT_HOST")
          jco-host (when jco-path (File. jco-path))
          ability {:target "clock://monotonic" :operation :clock/now
                   :max-bytes 1 :max-items 1 :deadline-ms 1000 :audit-id "component-e2e"}
          artifact (compiler/compile-component
                    "(ns app (:capabilities #{:clock/now})) (defn main [] (cap-call :clock/now 7))"
                    {:allow #{[:cap/call 7]}}
                    {:capability-mode :linear-resource
                     :component-abilities {7 ability}
                     :budgets {:fuel 100000 :memory-pages 4}})
          seen (atom [])
          providers {:aiueos.component/aiueos-clock-now
                     (fn [{:keys [payload] :as request}]
                       (swap! seen conj request)
                       (+ 100 (:value payload)))}
          wasmtime (run-on-host artifact host :wasmtime-component providers {})
          jco (when jco-host
                (run-on-host artifact jco-host :jco-component providers {}))]
      (testing "only the admitted named WIT import crosses the native host"
        (is (= :linear-resource (:capability-mode artifact)))
        (is (= {:result 107 :runtime :wasmtime-component}
               (select-keys wasmtime [:result :runtime])))
        (is (= {:import :aiueos.component/aiueos-clock-now
                :ability ability :payload {:value 7}}
               (first @seen))))
      (when jco
        (testing "the same effectful Component and authority run in independent hosts"
          (is (= {:result 107 :runtime :jco-component}
                 (select-keys jco [:result :runtime])))
          (is (= (adapter/portable-receipt wasmtime)
                 (adapter/portable-receipt jco)))
          (is (= (execution-identity (:bytes artifact))
                 (:execution-identity jco)
                 (:execution-identity wasmtime)))
          (is (not= (get-in wasmtime [:receipt :host])
                    (get-in jco [:receipt :host])))
          (is (= {:aiueos.component/aiueos-clock-now 1}
                 (get-in jco [:receipt :lease :consumed])))
          (is (= :linear-resource
                 (get-in jco [:receipt :capability-mode])
                 (get-in wasmtime [:receipt :capability-mode])))
          (is (= {:fuel 100000 :memory-pages 4 :deadline-ms 10000}
                 (get-in jco [:receipt :resource-bounds]))))
        (testing "live epoch revocation denies both engines before provider execution"
          (doseq [[runtime engine] [[:wasmtime-component host]
                                    [:jco-component jco-host]]]
            (let [calls (atom 0)
                  epoch-source #(if (= 1 (swap! calls inc)) 1 2)]
              (is (thrown-with-msg?
                   clojure.lang.ExceptionInfo #"lease denies"
                   (run-on-host artifact engine runtime providers
                                {:lease-epoch-source epoch-source}))))))
        (testing "deny-by-default policy is engine-independent"
          (doseq [[runtime engine] [[:wasmtime-component host]
                                    [:jco-component jco-host]]]
            (is (thrown? clojure.lang.ExceptionInfo
                         (run-on-host artifact engine runtime providers
                                      {:policy-overlay
                                       {:aiueos/require-signed true}})))))
        (testing "one-shot capability exhaustion is identical across engines"
          (let [twice (compiler/compile-component
                       "(ns app (:capabilities #{:clock/now}))
                        (defn main [] (do (cap-call :clock/now 7)
                                          (cap-call :clock/now 8)))"
                       {:allow #{[:cap/call 7]}}
                       {:component-abilities {7 ability}
                        :budgets {:fuel 100000 :memory-pages 5}})]
            (doseq [[runtime engine] [[:wasmtime-component host]
                                      [:jco-component jco-host]]]
              (is (thrown?
                   clojure.lang.ExceptionInfo
                   (run-on-host twice engine runtime providers {}))))))))
    ;; The executable is a deliberately separate native TCB.  Unit-test
    ;; invocations do not build it implicitly; CI sets this variable after
    ;; `cargo build --locked` and therefore cannot skip the integration path.
    (is true "KOTOTAMA_COMPONENT_HOST is not set; native integration test is CI-gated")))

(deftest ^:integration compiler-v03-typed-provider-round-trip
  (if-let [host-path (System/getenv "KOTOTAMA_COMPONENT_HOST")]
    (let [host (File. host-path)
          ability {:target "clock://monotonic" :operation :clock/now
                   :max-bytes 64 :max-items 1 :deadline-ms 1000
                   :audit-id "typed-v03-e2e"}
          artifact (compiler/compile-component
                    "(ns app (:capabilities #{:clock/now}))
                     (defn main [] (cap-call :clock/now 0))"
                    {:allow #{[:cap/call 7]}}
                    {:target abi/component-target-v2
                     :component-abilities {7 ability}
                     :budgets {:fuel 100000 :memory-pages 4
                               :deadline-ms 10000}})
          seen (atom nil)
          audited (atom nil)
          providers
          {:aiueos.component/aiueos-clock-now
           (fn [request]
             (reset! seen request)
             4242)}
          outcome
          (component/admit-and-run-with-aiueos!
           artifact
           (typed-component-world (:bytes artifact) (sha256-file host))
           (:bytes artifact)
           providers
           {:runtime :wasmtime-component
            :component-host (.getAbsolutePath host)
            :component-host-sha256 (sha256-file host)
            :now-ms (constantly 1000)
            :lease-epoch 1 :lease-ttl-ms 10000
            :audit-sink (fn [record]
                          (reset! audited record)
                          "persisted-typed-v03-e2e")})]
      (is (= abi/typed-capability-world-v3 (:component-world artifact)))
      (is (= {:result 4242 :runtime :wasmtime-component}
             (select-keys outcome [:result :runtime])))
      (is (= ability (:ability @seen)))
      (is (= "typed-v03-e2e" (:audit-id @audited)))
      (when-let [jco-path (System/getenv "KOTOTAMA_JCO_COMPONENT_HOST")]
        (let [jco-host (File. jco-path)
              jco-outcome
              (component/admit-and-run-with-aiueos!
               artifact
               (typed-component-world (:bytes artifact) (sha256-file jco-host))
               (:bytes artifact)
               providers
               {:runtime :jco-component
                :component-host (.getAbsolutePath jco-host)
                :component-host-sha256 (sha256-file jco-host)
                :now-ms (constantly 1000)
                :lease-epoch 1 :lease-ttl-ms 10000
                :audit-sink (fn [record]
                              (reset! audited record)
                              "persisted-typed-v03-jco")})]
          (is (= {:result 4242 :runtime :jco-component}
                 (select-keys jco-outcome [:result :runtime])))
          (is (= "typed-v03-e2e" (:audit-id @audited))))))
    (is true "KOTOTAMA_COMPONENT_HOST is not set; typed integration is CI-gated")))
